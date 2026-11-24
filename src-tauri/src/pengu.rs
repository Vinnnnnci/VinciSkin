//! Prepares the bundled Pengu Loader runtime for the League client.
//!
//! The client runs no local API the manager can reach, so the champ-select
//! pick has to be read from inside the client page. Pengu Loader provides that
//! runtime: Windows starts `rundll32` on the loader's `core.dll` in place of
//! `LeagueClientUx.exe` through the `Image File Execution Options` key, and the
//! DLL loads the plugin this module writes.
//!
//! The runtime ships beside the app (Pengu Loader is MIT licensed, its licence
//! is written beside the binaries) and is unpacked into the app's data
//! directory, so nothing is installed into the game or the client folder.

use std::path::{Path, PathBuf};

use fs_err as fs;
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

/// The runtime files, written into the app's data directory on demand.
const FILES: [(&str, &[u8]); 8] = [
    (
        "Pengu Loader.exe",
        include_bytes!("../resources/pengu/Pengu Loader.exe"),
    ),
    ("core.dll", include_bytes!("../resources/pengu/core.dll")),
    (
        "ModernWpf.dll",
        include_bytes!("../resources/pengu/ModernWpf.dll"),
    ),
    (
        "ModernWpf.Controls.dll",
        include_bytes!("../resources/pengu/ModernWpf.Controls.dll"),
    ),
    (
        "Ookii.Dialogs.Wpf.dll",
        include_bytes!("../resources/pengu/Ookii.Dialogs.Wpf.dll"),
    ),
    (
        "System.ValueTuple.dll",
        include_bytes!("../resources/pengu/System.ValueTuple.dll"),
    ),
    ("version", include_bytes!("../resources/pengu/version")),
    (
        "LICENSE.txt",
        include_bytes!("../resources/pengu/LICENSE.txt"),
    ),
];

/// The plugin the client page runs, with the bridge port and token filled in.
const PLUGIN_JS: &str = include_str!("../resources/lcu-bridge/index.js");

/// The DLL the client's debugger entry runs.
const CORE_DLL: &str = "core.dll";

/// The `Image File Execution Options` key the loader is registered under.
const IFEO_KEY: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\LeagueClientUx.exe";

/// Where the runtime lives: the app's own data directory.
fn runtime_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let data = crate::state::get_app_data_dir(app)
        .ok_or_else(|| AppError::Other("no app data directory for the client runtime".into()))?;
    Ok(data.join("runtime").join("pengu"))
}

/// What [`prepare`] wrote, and whether the client must restart to read it.
pub struct Prepared {
    /// Whether the debugger entry changed, which a running client reads only on restart.
    pub registration_changed: bool,
}

/// Write the runtime and the plugin, and point the client at the loader.
///
/// Called when the client bridge starts. It is idempotent: unchanged files are
/// left alone, so a running client is not disturbed by a restart of the app.
pub fn prepare(app: &AppHandle, port: u16, token: &str) -> AppResult<Prepared> {
    let dir = install_runtime(app)?;
    write_plugin(&dir, port, token)?;
    let registration_changed = enable(&dir)?;

    Ok(Prepared {
        registration_changed,
    })
}

/// Write every runtime file into `dir`, skipping ones that already match.
fn install_runtime(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = runtime_dir(app)?;
    fs::create_dir_all(&dir)?;

    for (name, bytes) in FILES {
        let path = dir.join(name);
        if fs::read(&path).ok().as_deref() != Some(bytes) {
            fs::write(&path, bytes)?;
        }
    }

    Ok(dir)
}

/// Write the plugin with this run's bridge port and token.
fn write_plugin(dir: &Path, port: u16, token: &str) -> AppResult<()> {
    let plugin_dir = dir.join("plugins").join("@vinciskin").join("bridge");
    fs::create_dir_all(&plugin_dir)?;

    let source = PLUGIN_JS
        .replace("__PORT__", &port.to_string())
        .replace("__TOKEN__", token);

    let path = plugin_dir.join("index.js");
    if fs::read_to_string(&path).ok().as_deref() != Some(source.as_str()) {
        fs::write(&path, source)?;
    }

    Ok(())
}

/// The client's debugger entry, as Pengu's own activation writes it.
fn debugger_value(dll: &Path) -> String {
    format!(r#"rundll32 "{}", #6000 "#, dll.display())
}

/// Whether two registered entries name the same command.
fn same_command(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Register the loader as the client's debugger, clearing the way first.
///
/// Answers whether the entry changed, which is what a running client needs a
/// restart for.
fn enable(dir: &Path) -> AppResult<bool> {
    let dll = dir.join(CORE_DLL);
    if !dll.is_file() {
        return Err(AppError::Other(
            "the bundled client loader is missing".into(),
        ));
    }

    let value = debugger_value(&dll);

    if let Some(existing) = registered()? {
        if !path_contains(&existing, dir) {
            return Err(AppError::Other(format!(
                "another client loader is already registered: {existing}"
            )));
        }

        if same_command(&existing, &value) {
            return Ok(false);
        }
    }

    /* The entry runs the DLL rather than the loader's window executable: the
    executable starts in place of the client and only shows its own window, so
    the client never comes up while it is the registered debugger. */
    run_elevated(
        "reg.exe",
        &format!(r#"add "{IFEO_KEY}" /v Debugger /t REG_SZ /d "{value}" /f"#),
    )?;

    match registered()? {
        Some(written) if same_command(&written, &value) => Ok(true),
        _ => Err(AppError::Other(
            "the client loader was not registered".into(),
        )),
    }
}

/// Remove the registration, leaving the client alone.
///
/// The key goes with its value: an entry left empty is the residue a value-only
/// delete leaves behind.
pub fn disable() -> AppResult<()> {
    if !key_present()? {
        return Ok(());
    }

    run_elevated("reg.exe", &format!(r#"delete "{IFEO_KEY}" /f"#))
}

/// Whether the client's debugger key exists, with or without a value.
fn key_present() -> AppResult<bool> {
    let output = std::process::Command::new("reg.exe")
        .args(["query", IFEO_KEY])
        .output()
        .map_err(|error| AppError::Other(format!("could not read the client registry: {error}")))?;

    Ok(output.status.success())
}

/// Whether a registered command line names a path inside `dir`.
///
/// The value can be `rundll32 "…\core.dll", #6000`, so the directory is looked
/// for as a substring rather than compared as a whole path.
fn path_contains(value: &str, dir: &Path) -> bool {
    let needle = dir.to_string_lossy();
    value.to_lowercase().contains(&needle.to_lowercase())
}

/// The loader the client is currently registered to start, if any.
pub fn registered() -> AppResult<Option<String>> {
    let output = std::process::Command::new("reg.exe")
        .args(["query", IFEO_KEY, "/v", "Debugger"])
        .output()
        .map_err(|error| AppError::Other(format!("could not read the client registry: {error}")))?;

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let value = text.lines().find_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0].eq_ignore_ascii_case("Debugger") {
            Some(parts[2..].join(" "))
        } else {
            None
        }
    });

    Ok(value)
}

/// Run a command elevated and wait for it, so the registry change is visible.
#[cfg(windows)]
fn run_elevated(exe: &str, args: &str) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };

    fn wide(text: &str) -> Vec<u16> {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let verb = wide("runas");
    let file = wide(exe);
    let params = wide(args);

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = 0;

    let started = unsafe { ShellExecuteExW(&mut info) };
    if started == 0 {
        return Err(AppError::Other(format!(
            "could not run {exe} elevated: {}",
            std::io::Error::last_os_error()
        )));
    }

    if info.hProcess.is_null() {
        return Err(AppError::Other(format!("{exe} gave no process handle")));
    }

    let mut code: u32 = 0;
    unsafe {
        WaitForSingleObject(info.hProcess as HANDLE, INFINITE);
        GetExitCodeProcess(info.hProcess as HANDLE, &mut code);
        CloseHandle(info.hProcess as HANDLE);
    }

    if code != 0 {
        return Err(AppError::Other(format!("{exe} exited with {code}")));
    }

    Ok(())
}

#[cfg(not(windows))]
fn run_elevated(_exe: &str, _args: &str) -> AppResult<()> {
    Err(AppError::Other("the client loader is Windows only".into()))
}

#[cfg(test)]
mod tests;
