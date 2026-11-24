//! The support report: what the app knows, collected into one zip a user can send.
//!
//! Nothing here walks a directory. Every input is a fact the app already holds - the checks that
//! just ran, the settings on disk, the library index, the patcher's status - plus the log files
//! the logger already wrote, and the whole bundle is redacted by
//! [`vinciskin_core::support_report`] before it is written.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::diagnostics::check_context;
use super::off_thread;
use crate::error::{AppError, AppResult, IpcResult};
use crate::mods::ModLibraryState;
use crate::patcher::PatcherState;
use crate::state::{IncidentStoreState, Settings, SettingsState};
use vinciskin_core::diagnostics::incident::Incident;
use vinciskin_core::support_report::{
    self, EnvironmentFacts, IncidentFile, LogSource, ModRow, PatcherFacts, ProblemKind,
    ReportInputs, SettingsSnapshot,
};

/// How much of one file the preview dialog may read out over IPC.
const PREVIEW_BYTES: usize = 96 * 1024;

/// How much of one log the report may read at all, which the builder trims again.
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// One file a report carries, with the text the preview shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportReportFile {
    pub path: String,
    pub bytes: usize,
    /// Whether the text below stops before the file does.
    pub truncated: bool,
    pub text: String,
}

/// The report as the preview dialog lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportReportPreview {
    pub summary: String,
    pub total_bytes: usize,
    pub suggested_name: String,
    pub files: Vec<SupportReportFile>,
}

/// What saving a report wrote.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSupportReport {
    pub path: String,
    pub bytes: u64,
    pub files: usize,
}

/// Builds the report without writing it, which is what the preview shows.
#[tauri::command]
pub async fn build_support_report(
    problem: String,
    notes: String,
    app: AppHandle,
) -> IpcResult<SupportReportPreview> {
    off_thread(move || preview(&app, &problem, &notes)).await
}

/// Writes the report to `path`, the one the save dialog answered.
#[tauri::command]
pub async fn save_support_report(
    path: String,
    problem: String,
    notes: String,
    app: AppHandle,
) -> IpcResult<SavedSupportReport> {
    off_thread(move || save(&app, &path, &problem, &notes)).await
}

fn preview(app: &AppHandle, problem: &str, notes: &str) -> AppResult<SupportReportPreview> {
    let report = build(app, problem, notes)?;

    let files = report
        .files()
        .iter()
        .map(|file| {
            let truncated = file.bytes.len() > PREVIEW_BYTES;
            let shown = &file.bytes[..file.bytes.len().min(PREVIEW_BYTES)];

            SupportReportFile {
                path: file.path.clone(),
                bytes: file.bytes.len(),
                truncated,
                text: String::from_utf8_lossy(shown).into_owned(),
            }
        })
        .collect();

    Ok(SupportReportPreview {
        summary: report.summary().to_owned(),
        total_bytes: report.total_bytes(),
        suggested_name: report.file_name(crate::logging::run_id()),
        files,
    })
}

fn save(app: &AppHandle, path: &str, problem: &str, notes: &str) -> AppResult<SavedSupportReport> {
    let report = build(app, problem, notes)?;
    let path = PathBuf::from(path);

    report
        .write_zip(&path)
        .map_err(|error| AppError::Other(error.to_string()))?;

    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);

    Ok(SavedSupportReport {
        path: path.to_string_lossy().into_owned(),
        bytes,
        files: report.files().len(),
    })
}

fn build(app: &AppHandle, problem: &str, notes: &str) -> AppResult<support_report::Report> {
    let inputs = collect(app, problem, notes);

    support_report::build(&inputs).map_err(|error| AppError::Other(error.to_string()))
}

/// Everything a report is built from, read once from the running app.
fn collect(app: &AppHandle, problem: &str, notes: &str) -> ReportInputs {
    let settings_state: tauri::State<'_, SettingsState> = app.state();
    let settings = settings_state.0.lock().clone();

    let diagnostics = vinciskin_core::diagnostics::run_all(&check_context(app, &settings_state));

    let library = {
        let library: tauri::State<'_, ModLibraryState> = app.state();
        library
            .0
            .get_installed_mods(&settings.config)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the report could not read the mod library");
                Vec::new()
            })
    };

    let patcher = app.state::<PatcherState>().status();

    ReportInputs {
        environment: environment(),
        diagnostics,
        settings: snapshot(&settings),
        library: library.iter().map(ModRow::from).collect(),
        patcher: patcher_facts(patcher),
        incidents: incidents(app),
        logs: logs(),
        problem: ProblemKind::parse(problem),
        notes: notes.to_owned(),
    }
}

/// Where the app is running, as the report states it.
fn environment() -> EnvironmentFacts {
    EnvironmentFacts {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        channel: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        os: vinciskin_core::diagnostics::windows::describe_os(),
        arch: std::env::consts::ARCH.to_owned(),
        webview2: vinciskin_core::diagnostics::windows::webview2_version(),
        run: crate::logging::run_id().map(str::to_owned),
        elevated: vinciskin_core::diagnostics::manager_is_elevated(),
    }
}

/// The settings the report may carry. The paths are written as placeholders by the redactor.
fn snapshot(settings: &Settings) -> SettingsSnapshot {
    SettingsSnapshot {
        league_path: path_text(settings.config.league_path.as_deref()),
        storage_path: path_text(settings.config.mod_storage_path.as_deref()),
        workshop_path: path_text(settings.config.workshop_path.as_deref()),
        theme: name_of(&settings.theme),
        launch_mode: name_of(&settings.launch_mode),
        auto_categorization: settings.config.auto_categorization_enabled,
        promote_enabled_mods: settings.config.promote_enabled_mods,
        hide_riot_client_on_launch: settings.config.hide_riot_client_on_launch,
        read_game_log: settings.config.read_game_log,
        keep_incidents: settings.config.keep_incidents,
        minimize_to_tray: settings.minimize_to_tray,
        start_in_tray: settings.start_in_tray,
        auto_run: settings.auto_run,
        watcher_enabled: settings.watcher_enabled,
        show_mod_tags: settings.show_mod_tags,
        verbose_patcher_logging: settings.verbose_patcher_logging,
        first_run_complete: settings.first_run_complete,
    }
}

fn patcher_facts(status: crate::patcher::PatcherStatus) -> PatcherFacts {
    PatcherFacts {
        running: status.running,
        state: status.state,
        error: status.error,
        game_running: status.game_running,
        loader_log: status
            .log_path
            .as_deref()
            .and_then(|path| path.rsplit(['\\', '/']).next())
            .map(str::to_owned),
        game: crate::logging::game_id(),
    }
}

/// The last few incidents, as the record and as the text a thread reads.
fn incidents(app: &AppHandle) -> Vec<IncidentFile> {
    let store = app.state::<IncidentStoreState>();

    let stored = match store.0.list() {
        Ok(incidents) => incidents,
        Err(error) => {
            tracing::warn!(%error, "the report could not read the incident store");
            return Vec::new();
        }
    };

    stored
        .iter()
        .take(5)
        .map(|incident: &Incident| IncidentFile {
            id: incident.id.clone(),
            json: serde_json::to_string_pretty(incident).unwrap_or_default(),
            text: incident.report_text(
                env!("CARGO_PKG_VERSION"),
                Some(&incident.token(env!("CARGO_PKG_VERSION"))),
                &[],
            ),
        })
        .collect()
}

/// The log files the logger already wrote, read tail-first.
fn logs() -> Vec<LogSource> {
    let Some(dir) = crate::logging::default_log_dir() else {
        return Vec::new();
    };

    let Ok(entries) = fs_err::read_dir(&dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            if !is_report_log(&name) {
                return None;
            }

            let metadata = entry.metadata().ok()?;
            let bytes = fs_err::read(entry.path()).ok()?;
            let bytes = tail(&bytes, MAX_LOG_BYTES as usize);

            Some(LogSource {
                name,
                text: String::from_utf8_lossy(&bytes).into_owned(),
                modified: metadata.modified().ok(),
            })
        })
        .collect()
}

/// Whether `name` is a log this app wrote, rather than anything else that ends in `.log`.
fn is_report_log(name: &str) -> bool {
    name.ends_with(".log")
        && (name.starts_with("vinciskin.")
            || name.starts_with("boot.")
            || name.starts_with("panic.")
            || name.starts_with("loader-"))
}

/// The last `room` bytes of `bytes`.
fn tail(bytes: &[u8], room: usize) -> &[u8] {
    &bytes[bytes.len().saturating_sub(room)..]
}

/// A path as text, which the redactor turns into its placeholder.
fn path_text(path: Option<&std::path::Path>) -> Option<String> {
    path.map(|path| path.to_string_lossy().into_owned())
}

/// The protocol name of a serde enum that serialises to a bare word.
fn name_of<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}
