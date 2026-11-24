//! Drives the cslol injection loader (`cslol-inj.exe`).
//!
//! The Chinese client's game runs at high integrity, and its module layout
//! breaks the bundled ltk patcher's integrity hook (`.rdata section not found`),
//! which aborts that DLL's init before its overlay hook is installed. cslol's
//! legacy loader has no such hook: it hooks `CreateFileA` and redirects the
//! game's WAD reads into the overlay, so it is the loader used here.
//!
//! It is launched elevated (`runas`), because a process at a lower integrity
//! level cannot hook the game (`Win32=5`). The process handle is kept so the
//! session can be stopped again, which a lower-integrity process could not do
//! with a fresh handle.
//!
//! The loader's console is hidden, so its output is redirected to a log file.
//! `status` reads that file and only reports `injected` once the DLL says its
//! initialization entry was reached.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fs_err as fs;
use parking_lot::Mutex;
use serde::Serialize;

use crate::error::{AppError, AppResult};

/// The bundled loader host.
pub const LOADER_EXE_NAME: &str = "cslol-inj.exe";
/// The bundled loader DLL, loaded from beside the host.
pub const LOADER_DLL_NAME: &str = "cslol-dll-legacy.dll";

/// The loader pair, embedded so a release build is one file.
const EMBEDDED_LOADER_EXE: &[u8] = include_bytes!("../resources/cslol-inj.exe");
const EMBEDDED_LOADER_DLL: &[u8] = include_bytes!("../resources/cslol-dll-legacy.dll");

/// The longest loader line a status carries.
const MAX_LOADER_ERROR: usize = 400;

/// Write the embedded loader into `dir`, answering the host's path.
///
/// A release build carries no `resources/` beside it, so the pair is unpacked
/// here on first use. Both files land together, because the host loads its DLL
/// from the directory it runs in.
pub fn unpack_loader(dir: &Path) -> AppResult<PathBuf> {
    fs::create_dir_all(dir)?;
    let host = dir.join(LOADER_EXE_NAME);

    write_if_changed(&host, EMBEDDED_LOADER_EXE)?;
    write_if_changed(&dir.join(LOADER_DLL_NAME), EMBEDDED_LOADER_DLL)?;

    Ok(host)
}

/// Write `bytes` to `path`, unless it already holds them.
fn write_if_changed(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if fs::read(path).is_ok_and(|held| held == bytes) {
        return Ok(());
    }
    fs::write(path, bytes)?;
    Ok(())
}

/// The state the patcher is in.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatcherStatus {
    /// Whether the loader process is alive.
    pub running: bool,
    /// `scanning`, `attached`, `injected` or `failed`, when there is a session.
    pub state: Option<String>,
    /// The last failure the loader reported.
    pub error: Option<String>,
    /// The log the loader writes, when a session has started.
    pub log_path: Option<String>,
    /// Whether the game process is up now.
    pub game_running: bool,
}

/// A process handle, kept so the session can be stopped again.
///
/// The job is what makes a stop reach the loader: the process is the hidden
/// `cmd` wrapper, and the loader is its child.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy)]
struct Handle {
    process: isize,
    job: isize,
}

// SAFETY: the value is an opaque Windows handle, only ever used through the
// Win32 calls that take it.
unsafe impl Send for Handle {}

#[cfg(windows)]
mod elevated {
    use super::{AppError, AppResult, Handle};
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, TerminateProcess};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    /// The exit code a process still running reports (`STILL_ACTIVE`).
    const STILL_ACTIVE: u32 = 259;

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Launch `exe` with `args` through the `runas` verb, answering its handle.
    pub fn spawn_runas(exe: &str, args: &str) -> AppResult<Handle> {
        let verb = wide("runas");
        let file = wide(exe);
        let params = wide(args);

        // SAFETY: every pointer below outlives the call, and the struct is
        // zeroed then filled the way `ShellExecuteExW` documents.
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        /* `SW_HIDE`: the loader writes its diagnostics to the redirected log,
        so its console is never shown. */
        info.nShow = 0;

        // SAFETY: `info` is a fully initialized SHELLEXECUTEINFOW.
        let started = unsafe { ShellExecuteExW(&mut info) };
        if started == 0 {
            return Err(AppError::Other(format!(
                "could not start the loader elevated: {}",
                std::io::Error::last_os_error()
            )));
        }
        if info.hProcess.is_null() {
            return Err(AppError::Other(
                "the loader started but gave no process handle".into(),
            ));
        }

        let job = attach_job(info.hProcess as HANDLE);

        Ok(Handle {
            process: info.hProcess as isize,
            job: job as isize,
        })
    }

    /// Put the wrapper in a job that ends its children when it is terminated.
    fn attach_job(process: HANDLE) -> HANDLE {
        // SAFETY: the job is created here, configured with a struct of the size
        // the call declares, and closed when it cannot hold the process.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                tracing::warn!("could not create a job for the injection loader");
                return std::ptr::null_mut();
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let configured = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0;
            let assigned = configured && AssignProcessToJobObject(job, process) != 0;

            if !assigned {
                tracing::warn!("could not put the injection loader in a job");
                CloseHandle(job);
                return std::ptr::null_mut();
            }

            job
        }
    }

    /// Whether the process behind `handle` is still running.
    pub fn alive(handle: Handle) -> bool {
        let mut code: u32 = 0;
        // SAFETY: `handle` is a live process handle.
        let ok = unsafe { GetExitCodeProcess(handle.process as HANDLE, &mut code) };
        ok != 0 && code == STILL_ACTIVE as u32
    }

    /// Stop the process behind `handle`, and its children.
    pub fn kill(handle: Handle) {
        // SAFETY: the process and job handles are live, and are closed once.
        unsafe {
            if handle.job != 0 {
                TerminateJobObject(handle.job as HANDLE, 0);
                CloseHandle(handle.job as HANDLE);
            } else {
                TerminateProcess(handle.process as HANDLE, 0);
            }
            CloseHandle(handle.process as HANDLE);
        }
    }
}

#[cfg(not(windows))]
mod elevated {
    use super::{AppError, AppResult, Handle};

    pub fn spawn_runas(_exe: &str, _args: &str) -> AppResult<Handle> {
        Err(AppError::Other(
            "the injection loader is Windows only".into(),
        ))
    }
    pub fn alive(_handle: Handle) -> bool {
        false
    }
    pub fn kill(_handle: Handle) {}
}

/// The processes the loader waits for and the one it is.
#[cfg(windows)]
mod probe {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    /// The game the loader hooks, as the process list names it.
    const GAME: &str = "League of Legends.exe";

    /// The running game's process id, which is what tells a session from a scan.
    pub fn game_pid() -> Option<u32> {
        find(GAME)
    }

    /// A loader process, whether or not this app started it.
    ///
    /// A loader left behind by a crashed or replaced app still injects, so a
    /// second one beside it would hook the game twice.
    pub fn loader_pid() -> Option<u32> {
        find(super::LOADER_EXE_NAME)
    }

    /// End the loader, which a `cmd` wrapper's handle does not reach.
    pub fn kill_loader() -> bool {
        let Some(pid) = loader_pid() else {
            return false;
        };

        // SAFETY: the handle is closed before returning, and only used through
        // the two Win32 calls that accept it.
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let killed = TerminateProcess(handle, 0) != 0;
            CloseHandle(handle);
            killed
        }
    }

    /// The first process whose executable name is `name`.
    fn find(name: &str) -> Option<u32> {
        // SAFETY: the snapshot is closed before returning, and the entry is
        // sized the way the API documents.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return None;
            }

            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            let mut found = None;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let listed: String = entry
                        .szExeFile
                        .iter()
                        .take_while(|unit| **unit != 0)
                        .map(|unit| char::from_u32(u32::from(*unit)).unwrap_or('\u{fffd}'))
                        .collect();
                    if listed.eq_ignore_ascii_case(name) {
                        found = Some(entry.th32ProcessID);
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }

            CloseHandle(snapshot);
            found
        }
    }
}

#[cfg(not(windows))]
mod probe {
    pub fn game_pid() -> Option<u32> {
        None
    }

    pub fn loader_pid() -> Option<u32> {
        None
    }

    pub fn kill_loader() -> bool {
        false
    }
}

/// What one loader run has read from its log.
struct Inner {
    handle: Option<Handle>,
    status: PatcherStatus,
    log_path: Option<PathBuf>,
    log_offset: u64,
    log_pending: String,
    saw_attached: bool,
    saw_injected: bool,
    saw_game: bool,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            handle: None,
            status: PatcherStatus::default(),
            log_path: None,
            log_offset: 0,
            log_pending: String::new(),
            saw_attached: false,
            saw_injected: false,
            saw_game: false,
        }
    }
}

/// The managed patcher state, held by Tauri for the commands.
#[derive(Default)]
pub struct PatcherState {
    inner: Arc<Mutex<Inner>>,
}

impl PatcherState {
    /// What the patcher is doing now, reaping a loader that has exited.
    pub fn status(&self) -> PatcherStatus {
        let mut inner = self.inner.lock();

        refresh_from_log(&mut inner);
        inner.status.game_running = probe::game_pid().is_some();

        /* A second game in one run gets an id of its own, so the reset is what the next
        launch is measured from rather than the first one. */
        if inner.status.game_running {
            if !inner.saw_game {
                inner.saw_game = true;
                crate::logging::start_game();
            }
        } else {
            inner.saw_game = false;
        }

        let alive = inner.handle.is_some_and(elevated::alive);
        if alive {
            inner.status.running = true;

            if inner.status.error.is_some() {
                inner.status.state = Some("failed".into());
            } else if inner.saw_injected {
                inner.status.state = Some("injected".into());
            } else if inner.saw_attached || inner.status.game_running {
                inner.status.state = Some("attached".into());
            } else {
                inner.status.state = Some("scanning".into());
            }
        } else {
            inner.handle = None;
            inner.status.running = false;
            inner.status.state = inner.status.error.as_ref().map(|_| "failed".to_string());
        }

        inner.status.clone()
    }

    /// Stop the loader.
    pub fn stop(&self) -> AppResult<()> {
        let mut inner = self.inner.lock();
        let Some(handle) = inner.handle else {
            return Err(AppError::Other("the patcher is not running".into()));
        };

        /* The loader cannot unload what it injected, and a running game is one
        it either hooked or is about to, so ending it mid-game leaves the hook
        in that process until the game closes. */
        if probe::game_pid().is_some() {
            return Err(AppError::Other(
                "the game is still running - close it before stopping the loader".into(),
            ));
        }

        inner.handle = None;
        elevated::kill(handle);
        if probe::loader_pid().is_some() && !probe::kill_loader() {
            tracing::warn!("the injection loader outlived its job");
        }
        inner.status = PatcherStatus {
            game_running: false,
            ..PatcherStatus::default()
        };
        crate::logging::end_session();
        tracing::info!("injection loader stopped");
        Ok(())
    }

    /// Start the loader over `overlay`, elevated.
    pub fn start(&self, loader_exe: &Path, overlay: &Path) -> AppResult<()> {
        let mut inner = self.inner.lock();
        if inner.handle.is_some_and(elevated::alive) {
            return Err(AppError::Other("the patcher is already running".into()));
        }
        inner.handle = None;

        /* A loader this app does not hold a handle on still hooks whatever game
        it finds, so one left by an earlier run has to be cleared first. */
        if probe::loader_pid().is_some() {
            return Err(AppError::Other(format!(
                "{} is already running - end it before starting another",
                LOADER_EXE_NAME
            )));
        }

        /* A game that is already up has opened its WADs, so a late injection
        either misses them or stacks a second hook in a process nothing ever
        unloads one from. The loader therefore only starts ahead of the game. */
        if probe::game_pid().is_some() {
            return Err(AppError::Other(
                "the game is already running - start the loader before launching it".into(),
            ));
        }

        let dll = loader_exe.with_file_name(LOADER_DLL_NAME);
        if !dll.is_file() {
            let error = format!(
                "{LOADER_DLL_NAME} is missing beside the loader; an antivirus may have removed it"
            );
            tracing::warn!(loader = %loader_exe.display(), "{error}");
            return Err(AppError::Other(error));
        }

        /* The watcher can write the first overlay after the host starts, so an
        empty overlay directory is a valid session. */
        fs::create_dir_all(overlay)?;

        let log_dir = crate::logging::default_log_dir()
            .ok_or_else(|| AppError::Other("no log directory for the injection loader".into()))?;
        fs::create_dir_all(&log_dir)?;

        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let log_path = log_dir.join(format!("loader-{stamp}.log"));
        let script_path = log_dir.join("loader-run.cmd");
        fs::write(&script_path, loader_script(loader_exe, overlay, &log_path))?;

        let command = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
        let arguments = format!("/c call \"{}\"", script_path.display());

        crate::logging::start_session();

        tracing::info!(
            loader = %loader_exe.display(),
            overlay = %overlay.display(),
            log = %log_path.display(),
            "starting the injection loader (elevated)"
        );

        let handle = match elevated::spawn_runas(&command, &arguments) {
            Ok(handle) => handle,
            Err(error) => {
                crate::logging::end_session();
                tracing::warn!(%error, "the injection loader did not start");
                return Err(error);
            }
        };

        inner.status = PatcherStatus {
            running: true,
            state: Some("scanning".into()),
            error: None,
            log_path: Some(log_path.to_string_lossy().into_owned()),
            game_running: false,
        };
        inner.log_path = Some(log_path);
        inner.log_offset = 0;
        inner.log_pending.clear();
        inner.saw_attached = false;
        inner.saw_injected = false;
        inner.saw_game = false;
        inner.handle = Some(handle);
        Ok(())
    }
}

/// The batch file that redirects the loader's console to its log.
///
/// `cmd` expands `%` in a batch line even inside quotes, so a path that holds
/// one is written doubled.
fn loader_script(loader: &Path, overlay: &Path, log: &Path) -> String {
    let path = |value: &Path| value.to_string_lossy().replace('%', "%%");

    format!(
        "@echo off\r\n\"{}\" \"{}\" > \"{}\" 2>&1\r\nexit /b\r\n",
        path(loader),
        path(overlay),
        path(log)
    )
}

/// Read the bytes written since the last check, and fold whole lines into the status.
fn refresh_from_log(inner: &mut Inner) {
    let Some(path) = inner.log_path.clone() else {
        return;
    };
    let Ok(bytes) = fs::read(&path) else {
        return;
    };
    if bytes.len() as u64 <= inner.log_offset {
        return;
    }

    let fresh = &bytes[inner.log_offset as usize..];
    inner.log_offset = bytes.len() as u64;
    inner.log_pending.push_str(&String::from_utf8_lossy(fresh));

    while let Some(end) = inner.log_pending.find('\n') {
        let line = inner.log_pending[..end].trim_end_matches('\r').to_string();
        inner.log_pending.drain(..=end);
        apply_loader_line(inner, &line);
    }
}

/// Fold one loader line into the session state.
fn apply_loader_line(inner: &mut Inner, line: &str) {
    let text = line.trim();
    if text.is_empty() {
        return;
    }

    let lower = text.to_ascii_lowercase();

    if is_loader_failure(&lower) {
        tracing::warn!(target: "patcher", loader = %text, "the injection loader reported a failure");
        inner.status.error = Some(clip(text, MAX_LOADER_ERROR));
        inner.status.state = Some("failed".into());
        return;
    }

    if lower.contains("dll initialization entry acknowledged") || lower.contains("init done") {
        tracing::info!(target: "patcher", "the injection DLL acknowledged its initialization");
        inner.saw_injected = true;
        inner.status.state = Some("injected".into());
        inner.status.error = None;
        return;
    }

    if lower.contains("[diag] target pid=") || lower.contains("waiting for game to exit") {
        inner.saw_attached = true;
        if !inner.saw_injected {
            inner.status.state = Some("attached".into());
        }
    }
}

/// Whether a loader line names a failure.
fn is_loader_failure(lower: &str) -> bool {
    [
        "failed to",
        "failed;",
        "timeout out",
        "no profile path",
        "invalid profile path",
        "loaded_dll=not loaded",
        "could not ",
        "error:",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// `text`, cut to `max` characters on a character boundary.
fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests;
