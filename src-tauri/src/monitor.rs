use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use chrono::Local;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::ime::{ImeStatus, ensure_english, ime_status};
use crate::process::{active_window_info, enumerate_gui_processes};
use crate::state::{FocusSnapshot, FocusSnapshotInternal, SharedAppState, StatusMessage};

pub struct Monitor {
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

const STATUS_COOLDOWN_MS: i64 = 1000;

impl Monitor {
    pub fn start(app: AppHandle, state: SharedAppState) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();

        let handle = thread::spawn(move || {
            if let Err(err) = run_loop(app, state, thread_shutdown) {
                tracing::error!(?err, "모니터링 스레드가 예외로 종료되었습니다");
            }
        });

        Self {
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_loop(app: AppHandle, state: SharedAppState, shutdown: Arc<AtomicBool>) -> Result<()> {
    while !shutdown.load(Ordering::Relaxed) {
        let (
            interval,
            use_auto_to_en,
            use_mouse_move,
            sensitivity,
            selected_processes,
            refresh_requested,
            last_cursor,
        ) = {
            let mut guard = state.lock();
            let cfg = guard.active_config();
            let interval = cfg.detect_interval_secs.max(0.1);
            let use_auto = cfg.use_auto_to_en;
            let use_mouse = cfg.use_mouse_move_event;
            let sensitivity = cfg.mouse_sensitivity;
            let selected = cfg.selected_processes.clone();
            let refresh_requested = guard.take_process_refresh_request();
            let last_cursor = guard.last_cursor_pos;
            (
                interval,
                use_auto,
                use_mouse,
                sensitivity,
                selected,
                refresh_requested,
                last_cursor,
            )
        };

        if refresh_requested {
            match enumerate_gui_processes() {
                Ok(list) => {
                    let mut guard = state.lock();
                    guard.set_available_processes(list.clone());
                    let _ = app.emit("processes-updated", list);
                }
                Err(err) => tracing::warn!(?err, "GUI 프로세스 목록을 가져오는 중 오류"),
            }
        }

        match active_window_info() {
            Ok(Some(active)) => {
                let ime = ime_status(active.hwnd).unwrap_or(ImeStatus::Unknown);

                let (prev_snapshot, manual_override_active) = {
                    let guard = state.lock();
                    (
                        guard.focus.clone(),
                        guard.manual_override_for(&active.process.name),
                    )
                };

                let mut manual_change = manual_override_active;
                let mut status_message: Option<StatusMessage> = None;
                let mut should_switch = false;
                let process_selected = selected_processes.contains(&active.process.name);

                if process_selected {
                    if let Some(prev) = prev_snapshot {
                        if let Some(prev_proc) = prev.process {
                            if prev_proc.name == active.process.name
                                && prev.ime_status == ImeStatus::English
                                && ime == ImeStatus::Original
                            {
                                manual_change = true;
                            }
                        }
                    }

                    if manual_change && ime == ImeStatus::English {
                        manual_change = false;
                    }

                    if use_auto_to_en
                        && matches!(ime, ImeStatus::Original | ImeStatus::Unknown)
                        && !manual_change
                    {
                        should_switch = true;
                    }
                }

                let mut new_cursor = last_cursor;
                if process_selected && use_mouse_move {
                    if let Some(position) = current_cursor_pos() {
                        new_cursor = Some(position);
                        if let Some(prev) = last_cursor {
                            if distance(prev, position) >= sensitivity {
                                manual_change = false;
                                if matches!(ime, ImeStatus::Original | ImeStatus::Unknown) {
                                    should_switch = true;
                                    status_message = Some(StatusMessage::with_values(
                                        "toast.status.mouseMove",
                                        [("name", active.process.name.clone())],
                                    ));
                                }
                            }
                        }
                    }
                }

                if should_switch {
                    match ensure_english(active.hwnd) {
                        Ok(toggled) => {
                            if toggled {
                                status_message = Some(StatusMessage::with_values(
                                    "toast.status.autoSwitch",
                                    [("name", active.process.name.clone())],
                                ));
                            }
                        }
                        Err(err) => {
                            tracing::warn!(?err, process = %active.process.name, "영문 전환 실패");
                        }
                    }
                }

                {
                    let mut guard = state.lock();
                    guard.set_manual_override(&active.process.name, manual_change);
                    guard.set_focus(Some(FocusSnapshotInternal {
                        process: Some(active.process.clone()),
                        ime_status: ime,
                        manual_override: manual_change,
                        updated_at: Local::now(),
                    }));
                    guard.last_cursor_pos = new_cursor;
                    let mut message_to_emit: Option<StatusMessage> = None;
                    if let Some(message) = status_message.clone() {
                        if guard.should_emit_status(&message, STATUS_COOLDOWN_MS) {
                            guard.record_status(message.clone());
                            message_to_emit = Some(message);
                        }
                    }
                    if let Some(snapshot) = guard.focus.as_ref() {
                        let _ = app.emit(
                            "focus-changed",
                            FocusSnapshot {
                                process: snapshot.process.clone(),
                                ime_status: snapshot.ime_status,
                                manual_override: snapshot.manual_override,
                                updated_at: Some(snapshot.updated_at.format("%H:%M:%S").to_string()),
                            },
                        );
                    }
                    drop(guard);
                    if let Some(message) = message_to_emit {
                        let _ = app.emit("status-message", message);
                    }
                }
            }
            Ok(None) => {
                let mut guard = state.lock();
                guard.set_focus(None);
                let _ = app.emit::<Option<FocusSnapshot>>("focus-changed", None);
            }
            Err(err) => tracing::warn!(?err, "활성 창 정보를 가져오는 중 오류"),
        }

        thread::sleep(Duration::from_secs_f32(interval));
    }

    Ok(())
}

fn current_cursor_pos() -> Option<(i32, i32)> {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_ok() {
        Some((point.x, point.y))
    } else {
        None
    }
}

fn distance(a: (i32, i32), b: (i32, i32)) -> f32 {
    let dx = (a.0 - b.0) as f32;
    let dy = (a.1 - b.1) as f32;
    (dx * dx + dy * dy).sqrt()
}
