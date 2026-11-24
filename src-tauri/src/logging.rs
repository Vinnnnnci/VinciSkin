//! The app log: one file a day, a boot timeline and a panic record.
//!
//! The file is the one a support report is read from, so the filter keeps a
//! default `warn` for crates no directive names. The boot timeline is written
//! without the subscriber, because a failure before logging is set up is the
//! one a user reports as "it does not start".

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};

use fs_err as fs;
use tracing_appender::{non_blocking, non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// The filter used when `RUST_LOG` names none.
const DEFAULT_FILTER: &str =
    "vinciskin=debug,ritoclient=debug,ritoclient_core=debug,ltk_overlay=info,tauri=info,warn";

/// The identifier the installer and the log directory use.
const IDENTIFIER: &str = "com.vinciskin.app";

/// The build at which Windows reports itself as Windows 11.
const WINDOWS_11_BUILD: u32 = 22_000;

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

    #[cfg(debug_assertions)]
    let app_handle_holder = Arc::new(OnceLock::new());

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| DEFAULT_FILTER.into());

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer());

    #[cfg(debug_assertions)]
    let registry = registry.with(crate::log_layer::TauriLogLayer::new(
        app_handle_holder.clone(),
    ));

    if let Some(writer) = writer {
        let file_layer = tracing_subscriber::fmt::layer()
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

/// Delete log files older than `max_age_days` from the log directory.
pub fn cleanup_old_logs(log_dir: &Path, max_age_days: u64) {
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);

    let entries = match fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read log directory for cleanup: {}", e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !is_log_file(&file_name) {
            continue;
        }

        let modified = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let age = match SystemTime::now().duration_since(modified) {
            Ok(d) => d,
            Err(_) => continue,
        };

        if age > max_age {
            if let Err(e) = fs::remove_file(&path) {
                tracing::warn!("Failed to delete old log file {}: {}", path.display(), e);
            } else {
                tracing::info!("Deleted old log file: {}", path.display());
            }
        }
    }
}

/// Whether `file_name` is one of the files this module writes.
fn is_log_file(file_name: &str) -> bool {
    file_name.ends_with(".log")
        && (file_name.starts_with("vinciskin.")
            || file_name.starts_with("loader-")
            || file_name == "boot.log"
            || file_name == "panic.log")
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

/// Build the rolling file writer, answering its guard, writer and directory.
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

    let file_appender = match rolling::RollingFileAppender::builder()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix("vinciskin")
        .filename_suffix("log")
        .build(&log_dir)
    {
        Ok(appender) => appender,
        Err(e) => {
            eprintln!(
                "Failed to create the log file in {}: {}",
                log_dir.display(),
                e
            );
            return (None, None, None);
        }
    };

    let (writer, guard) = non_blocking(file_appender);
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
            windows = windows_name(build),
            arch = std::env::consts::ARCH,
            webview2,
            "environment"
        );
        boot::stage_detail("webview2", webview2);

        if webview2 == "missing" {
            boot::stage("webview2-missing");
        } else if webview2
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .is_some_and(|major| major < 105)
        {
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

/// The Windows name a build number belongs to.
#[cfg(target_os = "windows")]
fn windows_name(build: u32) -> &'static str {
    if build >= WINDOWS_11_BUILD {
        "Windows 11"
    } else {
        "Windows 10"
    }
}

/// The Evergreen WebView2 runtime's version, from the registry either install writes.
#[cfg(target_os = "windows")]
fn webview2_version() -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };

    const MACHINE_KEY: &str =
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    const USER_KEY: &str =
        r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    fn read(root: HKEY, key: &str) -> Option<String> {
        let key = wide(key);
        let value = wide("pv");
        let mut buffer = [0u16; 128];
        let mut size = (buffer.len() * 2) as u32;

        // SAFETY: both strings are null-terminated, and `buffer` is `size`
        // bytes of writable storage.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 || size < 2 {
            return None;
        }

        let length = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buffer[..length]))
    }

    read(HKEY_LOCAL_MACHINE, MACHINE_KEY).or_else(|| read(HKEY_CURRENT_USER, USER_KEY))
}

/// A null-terminated UTF-16 string.
#[cfg(target_os = "windows")]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
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
                    file.write_all(line.as_bytes())?;
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

    /// Point the timeline at `dir`, once per process.
    pub fn init(dir: Option<&Path>) {
        let _ = PATH.set(dir.map(|dir| dir.join("boot.log")));
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
                file.write_all(entry.as_bytes())?;
                file.flush()
            });
    }
}

/// The timestamp every line of the boot and panic logs uses.
fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}
