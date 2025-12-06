#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(target_os = "windows"))]
compile_error!("Langcon은 Windows 전용 애플리케이션입니다.");

mod config;
mod ime;
mod monitor;
mod process;
mod state;
mod startup;

use std::sync::Arc;

use anyhow::{Result, anyhow};
use parking_lot::Mutex;
use tauri::{
    AppHandle,
    Emitter,
    LogicalSize,
    Manager,
    PhysicalPosition,
    PhysicalSize,
    Position,
    Size,
    State,
    WebviewWindow,
    WindowEvent,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent},
    Runtime,
};
use tauri_plugin_notification::NotificationExt;

use crate::config::{ConfigManager, WindowState};
use crate::ime::{ImeStatus, ime_status, toggle_hangul_key};
use crate::monitor::Monitor;
use crate::process::ActiveWindowInfo;
use crate::state::{AppViewModel, FocusSnapshot, FocusSnapshotInternal, SharedAppState};
use crate::config::{FALLBACK_LANGUAGE, sanitize_language};

const TRAY_MENU_SHOW: &str = "tray-show";
const TRAY_MENU_QUIT: &str = "tray-quit";
const TRAY_MENU_RESET_WINDOW: &str = "tray-reset-window";

struct TrayText {
    open: &'static str,
    reset_window: &'static str,
    quit: &'static str,
    running: &'static str,
}

fn tray_texts(language: &str) -> TrayText {
    match language {
        "ko" => TrayText {
            open: "창 열기",
            reset_window: "창 위치/크기 초기화",
            quit: "종료",
            running: "Langcon이 트레이에서 실행 중입니다.",
        },
        "ja" => TrayText {
            open: "ウィンドウを開く",
            reset_window: "ウィンドウ位置/サイズをリセット",
            quit: "終了",
            running: "Langcon がトレイで実行中です。",
        },
        "zh" => TrayText {
            open: "打开窗口",
            reset_window: "重置窗口位置/大小",
            quit: "退出",
            running: "Langcon 正在托盘中运行。",
        },
        _ => TrayText {
            open: "Open window",
            reset_window: "Reset window position/size",
            quit: "Quit",
            running: "Langcon is running in the tray.",
        },
    }
}

fn launched_from_autostart() -> bool {
    std::env::args().any(|arg| arg == crate::startup::AUTOSTART_FLAG)
}

fn notify_tray_running(app: &AppHandle, language: &str) {
    let texts = tray_texts(&sanitize_language(language));
    let _ = app
        .notification()
        .builder()
        .title("Langcon")
        .body(texts.running)
        .show();
}

struct AppContext {
    state: SharedAppState,
    monitor: Mutex<Monitor>,
    handle: AppHandle,
    config_manager: Arc<ConfigManager>,
}

impl AppContext {
    fn initialize(app: &AppHandle) -> Result<Self> {
        let (config_manager, config) = ConfigManager::load_or_create()?;
        let config_manager = Arc::new(config_manager);
        let state = Arc::new(Mutex::new(crate::state::AppState::new(
            config_manager.clone(),
            config,
        )));

        {
            let mut guard = state.lock();
            guard.request_process_refresh();
            guard.update_status_message(None);
            if guard.active_config().start_with_windows {
                if let Err(err) = crate::startup::set_autostart(true) {
                    tracing::warn!(?err, "시작 프로그램 등록에 실패했습니다");
                }
            }
        }

        let monitor = Monitor::start(app.clone(), state.clone());

        Ok(Self {
            state,
            monitor: Mutex::new(monitor),
            handle: app.clone(),
            config_manager,
        })
    }

    fn load_window_state(&self) -> Result<Option<WindowState>> {
        self.config_manager.load_window_state()
    }

    fn save_window_state(&self, state: WindowState) -> Result<()> {
        self.config_manager.save_window_state(&state)
    }
}

impl Drop for AppContext {
    fn drop(&mut self) {
        self.monitor.lock().stop();
    }
}

#[tauri::command]
fn load_state(app_state: State<AppContext>) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    Ok(guard.to_view_model())
}

#[tauri::command]
fn save_changes(app_state: State<AppContext>) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .save_changes()
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn discard_changes(app_state: State<AppContext>) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard.discard_changes();
    Ok(guard.to_view_model())
}

#[tauri::command]
fn set_use_auto_to_en(app_state: State<AppContext>, enabled: bool) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .set_use_auto_to_en(enabled)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn set_use_mouse_move_event(
    app_state: State<AppContext>,
    enabled: bool,
) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .set_use_mouse_move_event(enabled)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn set_detect_interval(app_state: State<AppContext>, seconds: f32) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .set_detect_interval(seconds)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn set_mouse_sensitivity(app_state: State<AppContext>, distance: f32) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .set_mouse_sensitivity(distance)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn set_start_with_windows(app_state: State<AppContext>, enabled: bool) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .set_start_with_windows(enabled)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn add_selected_process(app_state: State<AppContext>, name: String) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .add_selected_process(name)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn remove_selected_process(app_state: State<AppContext>, name: String) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard
        .remove_selected_process(&name)
        .map_err(|err| err.to_string())?;
    Ok(guard.to_view_model())
}

#[tauri::command]
fn refresh_processes(app_state: State<AppContext>) -> Result<AppViewModel, String> {
    match crate::process::enumerate_gui_processes() {
        Ok(list) => {
            {
                let mut guard = app_state.state.lock();
                guard.set_available_processes(list.clone());
            }
            let _ = app_state.handle.emit("processes-updated", list);
            let mut guard = app_state.state.lock();
            Ok(guard.to_view_model())
        }
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
fn toggle_ime(app_state: State<AppContext>) -> Result<FocusSnapshot, String> {
    let active = active_window()?
        .ok_or_else(|| "활성 창을 찾을 수 없습니다.".to_string())?;

    toggle_hangul_key().map_err(|err| err.to_string())?;
    let ime = ime_status(active.hwnd).unwrap_or(ImeStatus::Unknown);
    let snapshot = FocusSnapshotInternal {
        process: Some(active.process.clone()),
        ime_status: ime,
        manual_override: false,
        updated_at: chrono::Local::now(),
    };

    {
        let mut guard = app_state.state.lock();
        guard.set_focus(Some(snapshot.clone()));
    }

    let payload = FocusSnapshot {
        process: snapshot.process,
        ime_status: snapshot.ime_status,
        manual_override: snapshot.manual_override,
        updated_at: Some(snapshot.updated_at.format("%H:%M:%S").to_string()),
    };

    let _ = app_state.handle.emit("focus-changed", payload.clone());

    Ok(payload)
}

#[tauri::command]
fn set_manual_override(
    app_state: State<AppContext>,
    process_name: String,
    enabled: bool,
) -> Result<AppViewModel, String> {
    let mut guard = app_state.state.lock();
    guard.set_manual_override(&process_name, enabled);
    Ok(guard.to_view_model())
}

#[tauri::command]
fn set_language(app_state: State<AppContext>, language: String) -> Result<AppViewModel, String> {
    let language = sanitize_language(language);
    let mut guard = app_state.state.lock();
    guard
        .set_language(&language)
        .map_err(|err| err.to_string())?;
    drop(guard);
    apply_tray_language(&app_state.handle, &language);
    let mut guard = app_state.state.lock();
    Ok(guard.to_view_model())
}

#[tauri::command]
fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
async fn get_latest_version() -> Result<String, String> {
    let url = "https://raw.githubusercontent.com/0sami6/langcon/main/src-tauri/Cargo.toml";
    let resp = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("Failed to fetch latest version ({status})"));
    }
    let body = resp.text().await.map_err(|err| err.to_string())?;

    // Parse `version = "x.y.z"` from Cargo.toml without pulling full TOML parser.
    for line in body.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("version") {
            continue;
        }
        let mut parts = trimmed.splitn(2, '=');
        let key = parts.next().map(str::trim);
        let value = parts.next().map(str::trim);
        if key != Some("version") {
            continue;
        }
        if let Some(val) = value {
            let stripped = val.trim_matches(|c: char| c == '"' || c.is_whitespace());
            if !stripped.is_empty() {
                return Ok(stripped.to_string());
            }
        }
    }

    Err("Failed to parse version from Cargo.toml".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Err(err) = init_tracing() {
        eprintln!("로거 초기화 실패: {err}");
    }

    let autostart_launch = launched_from_autostart();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                    let app = window.app_handle();
                    let language = current_language(&app);
                    notify_tray_running(&app, &language);
                }
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    let app = window.app_handle();
                    if let Some(webview_window) = app.get_webview_window(window.label()) {
                        if let Err(err) = persist_window_state(&app, &webview_window) {
                            tracing::warn!(?err, "창 상태 저장에 실패했습니다");
                        }
                    }
                }
                _ => {}
            }
        })
        .setup(move |app| {
            let ctx = AppContext::initialize(&app.handle()).map_err(|err| err.to_string())?;
            if let Some(window) = app.get_webview_window("main") {
                if let Err(err) = restore_window_state(&ctx, &window) {
                    tracing::warn!(?err, "창 상태 복원에 실패했습니다");
                    let _ = window.center();
                }
                if autostart_launch {
                    let _ = window.hide();
                }
            }
            app.manage(ctx);
            let language = {
                let ctx = app.state::<AppContext>();
                let guard = ctx.state.lock();
                guard.current_language().to_string()
            };
            let tray = setup_tray(app, &language).map_err(|err| err.to_string())?;
            app.manage(tray);
            if autostart_launch {
                notify_tray_running(&app.handle(), &language);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_state,
            save_changes,
            discard_changes,
            set_use_auto_to_en,
            set_use_mouse_move_event,
            set_detect_interval,
            set_mouse_sensitivity,
            set_start_with_windows,
            add_selected_process,
            remove_selected_process,
            refresh_processes,
            toggle_ime,
            set_manual_override,
            set_language,
            get_app_version,
            get_latest_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_tracing() -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .try_init()
        .map_err(|err| anyhow!("로거 초기화 실패: {err}"))?;
    Ok(())
}

fn current_language(app: &AppHandle) -> String {
    if let Some(ctx) = app.try_state::<AppContext>() {
        let guard = ctx.state.lock();
        return guard.current_language().to_string();
    }
    FALLBACK_LANGUAGE.to_string()
}

fn active_window() -> std::result::Result<Option<ActiveWindowInfo>, String> {
    crate::process::active_window_info().map_err(|err| err.to_string())
}

fn persist_window_state(app: &AppHandle, window: &WebviewWindow) -> Result<()> {
    let position = window
        .outer_position()
        .map_err(|err| anyhow!("창 위치를 가져올 수 없습니다: {err}"))?;
    let size = window
        .outer_size()
        .map_err(|err| anyhow!("창 크기를 가져올 수 없습니다: {err}"))?;

    if let Some(ctx) = app.try_state::<AppContext>() {
        let state = WindowState {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        };
        ctx.save_window_state(state)?;
    }

    Ok(())
}

fn restore_window_state(ctx: &AppContext, window: &WebviewWindow) -> Result<()> {
    if let Some(state) = ctx.load_window_state()? {
        window
            .set_size(Size::Physical(PhysicalSize::new(state.width, state.height)))
            .map_err(|err| anyhow!("창 크기를 복원할 수 없습니다: {err}"))?;
        window
            .set_position(Position::Physical(PhysicalPosition::new(state.x, state.y)))
            .map_err(|err| anyhow!("창 위치를 복원할 수 없습니다: {err}"))?;
    } else {
        window
            .center()
            .map_err(|err| anyhow!("창을 중앙에 배치할 수 없습니다: {err}"))?;
    }
    Ok(())
}

fn reset_window(app: &AppHandle) -> Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_size(Size::Logical(LogicalSize::new(650.0, 800.0)))
            .map_err(|err| anyhow!("창 크기를 초기화할 수 없습니다: {err}"))?;
        window
            .center()
            .map_err(|err| anyhow!("창을 중앙에 배치할 수 없습니다: {err}"))?;

        persist_window_state(app, &window)?;
    }
    Ok(())
}

fn build_tray_menu<R: Runtime, M: Manager<R>>(app: &M, texts: &TrayText) -> tauri::Result<tauri::menu::Menu<R>> {
    let show_item = MenuItemBuilder::new(texts.open)
        .id(TRAY_MENU_SHOW)
        .build(app)?;
    let reset_window_item = MenuItemBuilder::new(texts.reset_window)
        .id(TRAY_MENU_RESET_WINDOW)
        .build(app)?;
    let quit_item = MenuItemBuilder::new(texts.quit)
        .id(TRAY_MENU_QUIT)
        .build(app)?;

    MenuBuilder::new(app)
        .item(&show_item)
        .separator()
        .item(&reset_window_item)
        .separator()
        .item(&quit_item)
        .build()
}

fn setup_tray(app: &mut tauri::App, language: &str) -> tauri::Result<TrayIcon> {
    let app_handle = app.handle();
    let texts = tray_texts(&sanitize_language(language));
    let tray_menu = build_tray_menu(app, &texts)?;

    let mut tray_builder = TrayIconBuilder::new()
        .menu(&tray_menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW => show_main_window(app),
            TRAY_MENU_RESET_WINDOW => {
                if let Err(err) = reset_window(app) {
                    tracing::warn!(?err, "창 위치/크기 초기화에 실패했습니다");
                } else {
                    show_main_window(app);
                }
            }
            TRAY_MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click { button, .. }
                if button == MouseButton::Left =>
            {
                show_main_window(tray.app_handle());
            }
            TrayIconEvent::DoubleClick { button, .. }
                if button == MouseButton::Left =>
            {
                show_main_window(tray.app_handle());
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    }

    tray_builder.build(app_handle)
}

fn apply_tray_language(app_handle: &AppHandle, language: &str) {
    let normalized = sanitize_language(language);
    let texts = tray_texts(&normalized);
    if let Some(tray) = app_handle.try_state::<TrayIcon>() {
        match build_tray_menu(app_handle, &texts) {
            Ok(menu) => {
                if let Err(err) = tray.set_menu(Some(menu)) {
                    tracing::warn!(?err, "트레이 메뉴를 업데이트하지 못했습니다");
                }
            }
            Err(err) => {
                tracing::warn!(?err, "트레이 메뉴 빌드에 실패했습니다");
            }
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(err) = window.show() {
            tracing::warn!(?err, "트레이에서 창을 표시하는 데 실패했습니다");
        }
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
