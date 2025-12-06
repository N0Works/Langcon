use std::collections::HashSet;

use anyhow::{Context, Result, anyhow};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, System};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible,
};

const BANNED_PROCESSES: &[&str] = &[
    "flet.exe",
    "explorer.exe",
    "textinputhost.exe",
    "nvidia overlay.exe",
    "systemsettings.exe",
    "applicationframehost.exe",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub title: String,
}

pub struct ActiveWindowInfo {
    pub hwnd: HWND,
    pub process: ProcessInfo,
}

pub fn enumerate_gui_processes() -> Result<Vec<ProcessInfo>> {
    let mut collector = ProcessCollector::default();
    unsafe {
        EnumWindows(
            Some(enum_windows_proc),
            LPARAM(&mut collector as *mut _ as isize),
        )
        .ok()
        .context("EnumWindows 호출 실패")?;
    }
    Ok(collector.finish())
}

pub fn active_window_info() -> Result<Option<ActiveWindowInfo>> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Ok(None);
    }

    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return Ok(None);
    }

    let process_name = process_name_for_pid(pid).context("프로세스 이름을 가져오지 못했습니다")?;
    let title = window_title(hwnd).unwrap_or_default();

    Ok(Some(ActiveWindowInfo {
        hwnd,
        process: ProcessInfo { pid, name: process_name, title },
    }))
}

#[derive(Default)]
struct ProcessCollector {
    entries: Vec<ProcessInfo>,
    seen_names: HashSet<String>,
}

impl ProcessCollector {
    fn push(&mut self, info: ProcessInfo) {
        let key = info.name.to_lowercase();
        if self.seen_names.insert(key) {
            self.entries.push(info);
        }
    }

    fn finish(mut self) -> Vec<ProcessInfo> {
        self.entries.sort_by(|a, b| a.name.cmp(&b.name));
        self.entries
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let collector = unsafe { &mut *(lparam.0 as *mut ProcessCollector) };

    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        let text_len = unsafe { GetWindowTextLengthW(hwnd) };
        if text_len > 0 {
            let mut pid = 0u32;
            unsafe {
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
            }
            if pid != 0 {
                if let Ok(name) = process_name_for_pid(pid) {
                    if is_banned(&name) {
                        return BOOL(1);
                    }
                    let title = window_title(hwnd).unwrap_or_default();
                    collector.push(ProcessInfo { pid, name, title });
                }
            }
        }
    }

    BOOL(1)
}

fn window_title(hwnd: HWND) -> Option<String> {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; (len + 1) as usize];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if copied <= 0 {
        return None;
    }
    buffer.truncate(copied as usize);
    Some(String::from_utf16_lossy(&buffer))
}

fn process_name_for_pid(pid: u32) -> Result<String> {
    let mut sys = PROCESS_SYSTEM.lock();
    let pid_sys = sys_pid_from_u32(pid);
    if !sys.refresh_process(pid_sys) {
        sys.refresh_processes();
    }
    sys.process(pid_sys)
        .map(|p| p.name().to_string())
        .ok_or_else(|| anyhow!("PID {}의 프로세스를 찾을 수 없습니다", pid))
}

fn is_banned(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    BANNED_PROCESSES.iter().any(|b| lower == *b)
}

static PROCESS_SYSTEM: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(System::new()));

fn sys_pid_from_u32(pid: u32) -> Pid {
    Pid::from(pid as usize)
}
