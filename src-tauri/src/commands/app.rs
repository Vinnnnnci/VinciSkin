use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use ts_rs::TS;

use crate::error::IpcResult;
use crate::state::SettingsState;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub log_file_path: Option<String>,
    pub os: String,
    pub arch: String,
}

/// Get basic app information.
#[tauri::command]
pub fn get_app_info() -> IpcResult<AppInfo> {
    let log_file_path =
        crate::logging::current_log_file().map(|path| path.to_string_lossy().into_owned());

    IpcResult::ok(AppInfo {
        name: "LTK Manager".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        log_file_path,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    })
}

/// A failure the frontend could not handle.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiErrorReport {
    pub message: String,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub route: Option<String>,
}

/// The window for a page that keeps failing, so one loop cannot fill the log.
static UI_ERRORS: OnceLock<Mutex<(Instant, u32)>> = OnceLock::new();

/// Whether this failure is inside the frontend's rate limit.
fn allow_ui_error() -> bool {
    let gate = UI_ERRORS.get_or_init(|| Mutex::new((Instant::now(), 0)));
    let mut gate = gate.lock();

    if gate.0.elapsed() > Duration::from_secs(60) {
        *gate = (Instant::now(), 0);
    }
    if gate.1 >= 30 {
        return false;
    }

    gate.1 += 1;
    true
}

/// Record a frontend failure in the app log.
#[tauri::command]
pub fn log_ui_error(report: UiErrorReport) -> IpcResult<()> {
    if !allow_ui_error() {
        return IpcResult::ok(());
    }

    let message = clip(&report.message, 4000);
    let stack = report
        .stack
        .as_deref()
        .map(|value| clip(value, 8000))
        .unwrap_or_default();

    tracing::error!(
        target: "ui",
        route = report.route.as_deref().unwrap_or(""),
        stack,
        "{message}"
    );

    IpcResult::ok(())
}

/// `value`, cut to `max` characters.
fn clip(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Reveal the main window once the frontend has finished its initial render.
///
/// The window is created hidden (`visible: false` in `tauri.conf.json`) to avoid a
/// white flash while the WebView loads. The frontend calls this after it mounts.
/// When the user has opted to start in the tray, the window stays hidden — the tray
/// icon reveals it later.
#[tauri::command]
pub fn show_main_window(app: AppHandle, settings: State<SettingsState>) -> IpcResult<()> {
    crate::logging::stage("ui-ready");

    let start_hidden = settings.0.lock().start_in_tray;

    if !start_hidden {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }

    IpcResult::ok(())
}
