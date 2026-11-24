//! The app log: one file a day, a boot timeline and a panic record.
//!
//! The file is the one a support report is read from, so the filter keeps a default `warn` for
//! crates no directive names. Every line carries the run it belongs to, and a session or game
//! where there is one. The boot timeline is written without the subscriber, because a failure
//! before logging is set up is the one a user reports as "it does not start".

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fs_err as fs;
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::{layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter};

mod format;
mod writer;

#[cfg(test)]
mod tests;

/// The filter used when `RUST_LOG` names none.
const DEFAULT_FILTER: &str =
    "vinciskin=debug,ritoclient=debug,ritoclient_core=debug,ltk_overlay=info,tauri=info,warn";

/// The directive the verbose setting raises, and the rung it raises it to.
const OVERLAY_DIRECTIVE: &str = "ltk_overlay=info";
const OVERLAY_VERBOSE: &str = "ltk_overlay=debug";

/// The identifier the installer and the log directory use.
const IDENTIFIER: &str = "com.vinciskin.app";

/// Everything the log directory may hold before the oldest files go.
const MAX_TOTAL_BYTES: u64 = 50 * 1024 * 1024;

/// The size `boot.log` is rotated at, which keeps one run's timeline from growing for days.
const MAX_BOOT_BYTES: u64 = 256 * 1024;

/// The run a line belongs to, set once per process.
static RUN: OnceLock<String> = OnceLock::new();

/// The session a line belongs to, `0` outside one.
static SESSION: AtomicU64 = AtomicU64::new(0);

/// The game a line belongs to, when a session found one.
static GAME: RwLock<Option<Arc<str>>> = RwLock::new(None);

/// The filter layer, replaced when the verbose setting changes.
static FILTER: OnceLock<reload::Handle<EnvFilter, tracing_subscriber::Registry>> = OnceLock::new();

/// Return value from logging initialization, holding guards that must live for the app lifetime.
pub struct LoggingGuards {
    pub _file_guard: Option<WorkerGuard>,
    pub log_path: Option<PathBuf>,
    #[cfg(debug_assertions)]
    pub app_handle_holder: Arc<OnceLock<tauri::AppHandle>>,
}

/// Set up the subscriber, the boot timeline and the panic record.
pub fn init() -> LoggingGuards {
    let (file_guard, writer, log_dir) = build_file_writer();
    boot::init(log_dir.as_deref());
    boot::stage("logging-ready");

    let _ = RUN.set(new_run_id());
    register_environment_paths();

    let text = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_FILTER.to_owned());
    let (filter, handle) = reload::Layer::new(EnvFilter::new(&text));
    let _ = FILTER.set(handle);

    #[cfg(debug_assertions)]
    let app_handle_holder = Arc::new(OnceLock::new());

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer());

    #[cfg(debug_assertions)]
    let registry = registry.with(crate::log_layer::TauriLogLayer::new(
        app_handle_holder.clone(),
    ));

    if let Some(writer) = writer {
        let file_layer = tracing_subscriber::fmt::layer()
            .event_format(format::LineFormat)
            .with_writer(writer)
            .with_ansi(false);
        registry.with(file_layer).init();
    } else {
        registry.init();
    }

    install_panic_hook(log_dir.clone());
    log_environment();

    LoggingGuards {
        _file_guard: file_guard,
        log_path: log_dir,
        #[cfg(debug_assertions)]
        app_handle_holder,
    }
}

/// Append one stage to the boot timeline.
pub fn stage(name: &str) {
    boot::stage(name);
}

/// Append one stage with a value to the boot timeline.
pub fn stage_detail(name: &str, detail: &str) {
    boot::stage_detail(name, detail);
}

/// Raise or lower the overlay's rung while the app runs.
///
/// `RUST_LOG` wins over the setting, because whoever set it is already choosing the filter.
pub fn set_verbose(enabled: bool) {
    if std::env::var_os("RUST_LOG").is_some() {
        return;
    }

    let Some(handle) = FILTER.get() else {
        return;
    };

    let text = if enabled {
        DEFAULT_FILTER.replace(OVERLAY_DIRECTIVE, OVERLAY_VERBOSE)
    } else {
        DEFAULT_FILTER.to_owned()
    };

    if let Err(error) = handle.reload(EnvFilter::new(text)) {
        tracing::warn!(%error, "could not change the log filter");
    }
}

/// The run every line of this process carries.
pub fn run_id() -> Option<&'static str> {
    RUN.get().map(String::as_str)
}

/// The session the lines from now on belong to, counting from one.
pub fn start_session() -> u64 {
    set_game(None);
    SESSION.fetch_add(1, Ordering::Relaxed) + 1
}

/// End the session, so lines from outside one carry no `session`.
pub fn end_session() {
    SESSION.store(0, Ordering::Relaxed);
    set_game(None);
}

/// Mark the game this run hooked, as the id its lines carry.
pub fn start_game() {
    let stamp = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    set_game(Some(stamp.into()));
}

/// Replace the game a line belongs to.
fn set_game(game: Option<Arc<str>>) {
    if let Ok(mut held) = GAME.write() {
        *held = game;
    }
}

/// The session a line belongs to, when one is open.
pub fn session_id() -> Option<u64> {
    match SESSION.load(Ordering::Relaxed) {
        0 => None,
        id => Some(id),
    }
}

/// The game a line belongs to, when a session found one.
pub fn game_id() -> Option<String> {
    GAME.read().ok()?.as_ref().map(|id| id.to_string())
}

/// The file the subscriber is writing, which is the newest log on disk.
pub fn current_log_file() -> Option<PathBuf> {
    let dir = default_log_dir()?;

    let newest = fs::read_dir(&dir).ok().and_then(|entries| {
        entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let name = name.to_str()?;
                if !name.starts_with("vinciskin.") || !name.ends_with(".log") {
                    return None;
                }

                let modified = entry.metadata().ok()?.modified().ok()?;
                Some((modified, entry.path()))
            })
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, path)| path)
    });

    newest.or_else(|| {
        Some(dir.join(format!(
            "vinciskin.{}.log",
            chrono::Local::now().format("%Y-%m-%d")
        )))
    })
}

/// Delete log files older than `max_age_days`, then the oldest until the directory fits.
pub fn cleanup_old_logs(log_dir: &Path, max_age_days: u64) {
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();
    let mut kept = Vec::new();

    for (modified, size, path) in log_files(log_dir) {
        let age = now.duration_since(modified).unwrap_or_default();

        if age > max_age {
            remove(&path);
        } else {
            kept.push((modified, size, path));
        }
    }

    trim_to_budget(kept, MAX_TOTAL_BYTES);
}

/// Delete the oldest of `kept` until the directory holds no more than `budget`.
fn trim_to_budget(kept: Vec<(SystemTime, u64, PathBuf)>, budget: u64) {
    let mut total: u64 = kept.iter().map(|(_, size, _)| size).sum();
    if total <= budget {
        return;
    }

    let mut kept = kept;
    kept.sort_by_key(|(modified, _, _)| *modified);

    for (_, size, path) in kept {
        if total <= budget {
            return;
        }

        remove(&path);
        total = total.saturating_sub(size);
    }
}

/// Delete one log file, reporting a failure that left it behind.
fn remove(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => tracing::info!("Deleted old log file: {}", path.display()),
        Err(error) => tracing::warn!(
            "Failed to delete old log file {}: {}",
            path.display(),
            error
        ),
    }
}

/// Every file this module wrote, as its modified time, size and path.
fn log_files(log_dir: &Path) -> Vec<(SystemTime, u64, PathBuf)> {
    let entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!("Failed to read log directory for cleanup: {}", error);
            return Vec::new();
        }
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            if !is_log_file(name.to_str()?) {
                return None;
            }

            let metadata = entry.metadata().ok()?;

            Some((metadata.modified().ok()?, metadata.len(), entry.path()))
        })
        .collect()
}

/// Whether `file_name` is one of the files this module writes.
fn is_log_file(file_name: &str) -> bool {
    if !file_name.ends_with(".log") {
        return false;
    }

    file_name.starts_with("vinciskin.")
        || file_name.starts_with("loader-")
        || file_name.starts_with("boot.")
        || file_name.starts_with("panic.")
}

/// The directory the log files live in.
pub(crate) fn default_log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(PathBuf::from(appdata).join(IDENTIFIER).join("logs"));
    }

    #[cfg(target_os = "macos")]
    if let Ok(home) = std::env::var("HOME") {
        return Some(
            PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join(IDENTIFIER),
        );
    }

    #[cfg(target_os = "linux")]
    if let Ok(home) = std::env::var("HOME") {
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(IDENTIFIER)
                .join("logs"),
        );
    }

    Some(std::env::temp_dir().join("vinciskin"))
}

/// Register the folders a user's account names, so no line has to carry them.
fn register_environment_paths() {
    for (name, placeholder) in [
        ("USERPROFILE", "%USERPROFILE%"),
        ("APPDATA", "%APPDATA%"),
        ("LOCALAPPDATA", "%LOCALAPPDATA%"),
    ] {
        if let Some(value) = std::env::var_os(name) {
            vinciskin_core::redact::register(&value.to_string_lossy(), placeholder);
        }
    }
}

/// Build the file writer, answering its guard, writer and directory.
fn build_file_writer() -> (
    Option<WorkerGuard>,
    Option<non_blocking::NonBlocking>,
    Option<PathBuf>,
) {
    let log_dir = match default_log_dir() {
        Some(d) => d,
        None => return (None, None, None),
    };

    if let Err(e) = fs::create_dir_all(&log_dir) {
        eprintln!(
            "Failed to create log directory {}: {}",
            log_dir.display(),
            e
        );
        return (None, None, None);
    }

    let sink = writer::RedactingWriter::new(writer::DailyFile::new(log_dir.clone()));
    let (writer, guard) = non_blocking(sink);

    (Some(guard), Some(writer), Some(log_dir))
}

/// Log the facts a support report asks for first.
fn log_environment() {
    #[cfg(target_os = "windows")]
    {
        let (major, minor, build) = os_version();
        let webview2 = webview2_version();
        let webview2 = webview2.as_deref().unwrap_or("missing");

        tracing::info!(
            os = %format!("{major}.{minor}.{build}"),
            windows = vinciskin_core::diagnostics::windows::windows_name(build),
            arch = std::env::consts::ARCH,
            webview2,
            "environment"
        );
        boot::stage_detail("webview2", webview2);

        if webview2 == "missing" {
            boot::stage("webview2-missing");
        } else if vinciskin_core::diagnostics::windows::webview2_too_old(webview2) {
            tracing::warn!(
                webview2,
                "WebView2 is older than the frontend's chrome105 target"
            );
            boot::stage_detail("webview2-old", webview2);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        tracing::info!(
            os = std::env::consts::OS,
            arch = std::env::consts::ARCH,
            "environment"
        );
    }
}

/// The OS major, minor and build from the kernel-shared data page.
#[cfg(target_os = "windows")]
fn os_version() -> (u32, u32, u32) {
    // SAFETY: KUSER_SHARED_DATA is mapped read-only at 0x7FFE0000 on every
    // Windows process; these are its documented offsets.
    unsafe {
        let build = std::ptr::read_volatile((0x7ffe0000 + 0x260) as *const u32);
        let major = std::ptr::read_volatile((0x7ffe0000 + 0x26c) as *const u32);
        let minor = std::ptr::read_volatile((0x7ffe0000 + 0x270) as *const u32);
        (major, minor, build)
    }
}

/// The Evergreen WebView2 runtime's version, as the diagnostics read it too.
#[cfg(target_os = "windows")]
fn webview2_version() -> Option<String> {
    vinciskin_core::diagnostics::windows::webview2_version()
}

/// Write a panic line to the log and to `panic.log` before the process ends.
fn install_panic_hook(log_dir: Option<PathBuf>) {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let message = panic_message(info);
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_default();

        /* The file comes first: a panic raised while the subscriber is
        formatting a line must not be the one that loses its own record. */
        if let Some(dir) = &log_dir {
            let stamp = now();
            let line = format!("{stamp} panic at {location}: {message}\n");
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("panic.log"))
                .and_then(|mut file| {
                    file.write_all(vinciskin_core::redact::text(&line).as_bytes())?;
                    file.flush()
                });
        }

        tracing::error!(target: "panic", location = %location, "{message}");

        default_hook(info);
    }));
}

/// A panic payload, as the text a log line carries.
fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();

    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "a panic with no message".to_owned()
    }
}

/// The boot timeline, which does not need the subscriber to work.
mod boot {
    use super::*;

    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

    /// Point the timeline at `dir`, once per process, rotating a file too large to read.
    pub fn init(dir: Option<&Path>) {
        let path = dir.map(|dir| dir.join("boot.log"));

        if let Some(path) = &path {
            rotate_if_large(path);
        }

        let _ = PATH.set(path);
    }

    /// Append one stage.
    pub fn stage(name: &str) {
        write(&format!("stage={name}"));
    }

    /// Append one stage and its value.
    pub fn stage_detail(name: &str, detail: &str) {
        let detail = detail
            .replace('\n', " ")
            .replace('\r', " ")
            .replace('"', "'");
        write(&format!("stage={name} detail=\"{detail}\""));
    }

    /// Keep the timeline to the last run, by moving a file past the cap aside.
    fn rotate_if_large(path: &Path) {
        let size = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        if size <= MAX_BOOT_BYTES {
            return;
        }

        let rotated = path.with_file_name("boot.1.log");
        let _ = fs::remove_file(&rotated);
        let _ = fs::rename(path, rotated);
    }

    fn write(line: &str) {
        let Some(path) = PATH.get().and_then(Option::as_ref) else {
            return;
        };

        let entry = format!("{} {line}\n", now());
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| {
                file.write_all(vinciskin_core::redact::text(&entry).as_bytes())?;
                file.flush()
            });
    }
}

/// The timestamp every line carries, local time so it matches the file it is in.
fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}

/// A random-looking id for this run, narrow enough to read in a line.
fn new_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    let pid = u64::from(std::process::id());

    /* A 32-bit mix of the two: the id only has to tell this process's lines from the last
    run's, and eight hex digits is what a person can carry into an issue. */
    let mut mixed = nanos ^ (pid << 32);
    mixed ^= mixed >> 33;
    mixed = mixed.wrapping_mul(0xff51_afd7_ed55_8ccd);
    mixed ^= mixed >> 33;

    format!("{:08x}", mixed as u32)
}
