//! Finds the game window and asks the cslol legacy DLL to hook its thread.
//!
//! `cslol-inj.exe` did this before, but it picked the target thread itself and
//! could pick one that never pumps messages, which left the DLL unloaded from
//! the game and the overlay unserved. The DLL's `cslol_hook` takes a thread id
//! and nothing else, so the choice is made here: only a window of
//! `League of Legends.exe` is offered, and only once that window answers.
//!
//! The DLL is the payload as well as the API, so it is loaded from disk and
//! never touched. Every line this program prints goes to the log the app reads
//! back, prefixed `[inj]` or `[dll]`.

#[cfg(windows)]
mod win {
    use std::ffi::{c_char, CStr};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, BOOL, HWND, INVALID_HANDLE_VALUE, LPARAM,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, SendMessageTimeoutW, SMTO_ABORTIFHUNG, SMTO_BLOCK, WM_NULL,
    };

    /// The executable the loader hooks.
    const GAME_EXE: &str = "League of Legends.exe";

    /// The window the game is expected to show, matched case-insensitively.
    const GAME_WINDOW: &str = "league of legends";

    /// The functions the legacy DLL exports and this host calls.
    pub struct Api {
        init: unsafe extern "C" fn() -> *const c_char,
        set_config: unsafe extern "C" fn(*const u16) -> *const c_char,
        set_log_level: unsafe extern "C" fn(u32) -> *const c_char,
        hook: unsafe extern "C" fn(u32, u32, u32) -> *const c_char,
        log_pull: unsafe extern "C" fn() -> *const c_char,
        find: unsafe extern "C" fn() -> u32,
    }

    impl Api {
        /// Load `path` and resolve every export the session needs.
        pub fn load(path: &Path) -> Result<Self, String> {
            let wide = wide(path.as_os_str().to_string_lossy().as_ref());
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            if module.is_null() {
                return Err(format!(
                    "cannot load {} (Win32 {})",
                    path.display(),
                    // SAFETY: no pointer is involved.
                    unsafe { GetLastError() }
                ));
            }

            // SAFETY: every name is NUL-terminated, and `module` is the live handle above.
            let get = |name: &str| -> *const () {
                let name = std::ffi::CString::new(name).expect("a NUL-free symbol name");
                unsafe { GetProcAddress(module, name.as_ptr().cast()) }
                    .map(|address| address as *const ())
                    .unwrap_or(std::ptr::null())
            };

            macro_rules! export {
                ($name:literal) => {{
                    let address = get($name);
                    if address.is_null() {
                        return Err(format!("{} does not export {}", path.display(), $name));
                    }
                    // SAFETY: the address came from the DLL's export table under this name.
                    unsafe { std::mem::transmute(address) }
                }};
            }

            Ok(Api {
                init: export!("cslol_init"),
                set_config: export!("cslol_set_config"),
                set_log_level: export!("cslol_set_log_level"),
                hook: export!("cslol_hook"),
                log_pull: export!("cslol_log_pull"),
                find: export!("cslol_find"),
            })
        }

        /// Point the DLL at `overlay`, keeping every log line it produces.
        pub fn configure(&self, overlay: &Path) -> Result<(), String> {
            // SAFETY: both calls are the DLL's own exported entry points, and the
            // string below stays alive for the duration of the call.
            unsafe {
                if let Some(error) = cerror((self.init)()) {
                    return Err(error);
                }
                (self.set_log_level)(0x1000);
            }

            let prefix: Vec<u16> = overlay
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            // SAFETY: `prefix` is a NUL-terminated UTF-16 string.
            match unsafe { cerror((self.set_config)(prefix.as_ptr())) } {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }

        /// Hook `tid`, waiting `timeout` ms for the DLL to load in the game.
        pub fn hook(&self, tid: u32, timeout: u32, step: u32) -> Result<(), String> {
            // SAFETY: `tid` came from a window of the game process.
            match unsafe { cerror((self.hook)(tid, timeout, step)) } {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }

        /// The next line the DLL logged, from either process.
        pub fn next_log(&self) -> Option<String> {
            // SAFETY: the DLL owns the buffer and hands back a NUL-terminated string.
            unsafe { cerror((self.log_pull)()) }
        }

        /// The thread the DLL would pick on its own, for a comparison in the log.
        pub fn find(&self) -> u32 {
            // SAFETY: the export takes no arguments.
            unsafe { (self.find)() }
        }
    }

    /// `text` as a Rust string, `None` for a null pointer.
    ///
    /// The DLL returns null for success, so a null result reads as absence
    /// rather than as the empty message it also could be.
    unsafe fn cerror(text: *const c_char) -> Option<String> {
        if text.is_null() {
            return None;
        }
        Some(
            unsafe { CStr::from_ptr(text) }
                .to_string_lossy()
                .into_owned(),
        )
    }

    /// Where the game is and which of its threads the hook goes to.
    pub struct Target {
        pub pid: u32,
        pub tid: u32,
        pub title: String,
        pub exe: String,
    }

    /// Wait for a window of `exe` that answers messages, up to `give_up`.
    pub fn find_target(exe: &str, give_up: Duration) -> Option<Target> {
        let deadline = std::time::Instant::now() + give_up;

        while std::time::Instant::now() < deadline {
            for pid in pids_of(exe) {
                let Some((hwnd, tid, title)) = window_of(pid) else {
                    continue;
                };
                if !answers(hwnd) {
                    continue;
                }

                return Some(Target {
                    pid,
                    tid,
                    title,
                    exe: process_path(pid).unwrap_or_default(),
                });
            }

            sleep(200);
        }

        None
    }

    /// Whether `pid` still exists, which is how long the session lasts.
    pub fn process_alive(pid: u32) -> bool {
        // SAFETY: the handle is closed once, and a null answer means the process is gone.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            CloseHandle(handle);
        }
        true
    }

    /// Every running process whose executable is `exe`.
    fn pids_of(exe: &str) -> Vec<u32> {
        let mut pids = Vec::new();

        // SAFETY: the snapshot handle is closed before returning, and the entry
        // struct is handed to the API with the size the API requires.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
                return pids;
            }

            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if name.eq_ignore_ascii_case(exe) {
                    pids.push(entry.th32ProcessID);
                }
                more = Process32NextW(snapshot, &mut entry) != 0;
            }

            CloseHandle(snapshot);
        }

        pids
    }

    /// What one `EnumWindows` pass collects.
    struct Search {
        pid: u32,
        best: Option<(i32, HWND, u32, String)>,
    }

    /// The best window `pid` owns, scored by how much it looks like the game.
    fn window_of(pid: u32) -> Option<(HWND, u32, String)> {
        let mut search = Search { pid, best: None };

        // SAFETY: the callback only writes into `search`, which outlives the call.
        unsafe {
            EnumWindows(Some(visit), &mut search as *mut Search as LPARAM);
        }

        search.best.map(|(_, hwnd, tid, title)| (hwnd, tid, title))
    }

    /// Score one top-level window, keeping the best so far.
    ///
    /// An answer that names the game outranks one that is merely visible: the
    /// process can own tool windows of its own, and the hook must land on the
    /// thread that runs the frame loop.
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Search` `window_of` passes.
        let search = unsafe { &mut *(lparam as *mut Search) };
        let mut pid = 0u32;

        // SAFETY: `hwnd` is a live window handle and `pid` is a local.
        let tid = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if tid == 0 || pid != search.pid {
            return 1;
        }

        // SAFETY: the length is read before the text, and the buffer matches it.
        let text = unsafe {
            let length = GetWindowTextLengthW(hwnd);
            if length <= 0 {
                return 1;
            }
            let mut buffer = vec![0u16; length as usize + 1];
            let written = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            buffer.truncate(written.max(0) as usize);
            String::from_utf16_lossy(&buffer)
        };

        // SAFETY: `hwnd` is a live window handle.
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        let named = text.to_lowercase().contains(GAME_WINDOW);
        let score = i32::from(visible) + if named { 2 } else { 0 };

        if search.best.as_ref().is_none_or(|(best, ..)| score > *best) {
            search.best = Some((score, hwnd, tid, text));
        }

        1
    }

    /// Whether the window answers a message within `SMTO`'s budget.
    ///
    /// A thread that does not pump leaves the posted message in its queue, so a
    /// window that never answers is a window whose thread the hook cannot reach.
    fn answers(hwnd: HWND) -> bool {
        let mut result = 0usize;

        // SAFETY: the message carries no pointers, and `result` is a local.
        let answered = unsafe {
            SendMessageTimeoutW(
                hwnd,
                WM_NULL,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                1000,
                &mut result,
            )
        };

        answered != 0
    }

    /// The full path of `pid`'s executable, when it can be read.
    fn process_path(pid: u32) -> Option<String> {
        // SAFETY: the handle is closed once, and both buffers are sized for the call.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }

            let mut buffer = [0u16; 1024];
            let mut length = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length);
            CloseHandle(handle);

            if ok == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&buffer[..length as usize]))
        }
    }

    /// `text` as NUL-terminated UTF-16.
    pub fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Sleep for `ms` milliseconds.
    pub fn sleep(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    /// The command line, as one struct so a missing flag is one check.
    pub struct Args {
        pub dll: PathBuf,
        pub overlay: PathBuf,
        /// Print what would be targeted and exit, without hooking anything.
        pub probe: bool,
        /// The executable to look for, which only a diagnostic overrides.
        pub exe: String,
    }

    /// Parse `--dll`, `--overlay`, `--probe` and the optional `--exe`.
    pub fn args() -> Result<Args, String> {
        let mut dll = None;
        let mut overlay = None;
        let mut probe = false;
        let mut exe = None;

        let mut words = std::env::args().skip(1);
        while let Some(word) = words.next() {
            match word.as_str() {
                "--dll" => dll = words.next(),
                "--overlay" => overlay = words.next(),
                "--exe" => exe = words.next(),
                "--probe" => probe = true,
                other => return Err(format!("unknown argument {other}")),
            }
        }

        match (dll, overlay) {
            (Some(dll), Some(overlay)) => Ok(Args {
                dll: dll.into(),
                overlay: overlay.into(),
                probe,
                exe: exe.unwrap_or_else(|| GAME_EXE.to_owned()),
            }),
            _ => Err("usage: vinciskin-inj --dll <path> --overlay <path>".into()),
        }
    }
}

/// Print one line to the log the app reads back.
fn say(tag: &str, text: &str) {
    println!("[{tag}] {text}");
}

/// Entry point: configure the DLL, wait for the game, hook it, then follow it.
#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use win::Api;

    /// How many times one game's window is offered to the hook before giving up.
    const MAX_ATTEMPTS: u32 = 5;

    let args = match win::args() {
        Ok(args) => args,
        Err(usage) => {
            say("inj", &format!("failed: {usage}"));
            return std::process::ExitCode::FAILURE;
        }
    };

    say(
        "inj",
        &format!(
            "opening {} for {}",
            args.dll.display(),
            args.overlay.display()
        ),
    );

    let api = match Api::load(&args.dll) {
        Ok(api) => api,
        Err(error) => {
            say("inj", &format!("failed: {error}"));
            return std::process::ExitCode::FAILURE;
        }
    };

    if let Err(error) = api.configure(&args.overlay) {
        say("inj", &format!("failed: {error}"));
        return std::process::ExitCode::FAILURE;
    }
    say("inj", "ready");

    if args.probe {
        /* The DLL's own finder is kept only for the comparison it prints, and
        the target below is the one a session would hook. */
        say("inj", &format!("dll finder says tid={}", api.find()));

        match win::find_target(&args.exe, std::time::Duration::from_secs(5)) {
            Some(target) => say(
                "inj",
                &format!(
                    "would target pid={} tid={} window={:?} exe={:?}",
                    target.pid, target.tid, target.title, target.exe
                ),
            ),
            None => say("inj", "would target nothing: no window answered"),
        }

        return std::process::ExitCode::SUCCESS;
    }

    loop {
        say("inj", "waiting for game");

        let mut attempts = 0;

        let Some(target) = win::find_target(&args.exe, std::time::Duration::from_secs(30 * 60))
        else {
            say("inj", "failed: no game window answered in 30 minutes");
            return std::process::ExitCode::FAILURE;
        };

        say(
            "inj",
            &format!(
                "game found pid={} tid={} window={:?} exe={:?}",
                target.pid, target.tid, target.title, target.exe
            ),
        );

        let started = std::time::Instant::now();
        match api.hook(target.tid, 90_000, 100) {
            Err(error) => {
                attempts += 1;
                say(
                    "inj",
                    &format!("failed attempt {attempts} of {MAX_ATTEMPTS}: {error}"),
                );

                /* A window that answered can still hand the hook to a thread
                that exits before the DLL loads, so the target is looked up
                again rather than the session ending on the first miss. */
                if attempts >= MAX_ATTEMPTS {
                    say("inj", &format!("failed: {error}"));
                    return std::process::ExitCode::FAILURE;
                }

                win::sleep(1000);
                continue;
            }
            Ok(()) => say(
                "inj",
                &format!("dll acknowledged in {} ms", started.elapsed().as_millis()),
            ),
        }

        /* The DLL reports a failed patch on the line after it acknowledges, so
        the verdict is read from its own log rather than assumed from the ack. */
        let settled = settle(&api);
        match settled {
            Some(error) => {
                say("inj", &format!("failed: {error}"));
                return std::process::ExitCode::FAILURE;
            }
            None => say("inj", "injected"),
        }

        while win::process_alive(target.pid) {
            drain(&api);
            win::sleep(500);
        }

        drain(&api);
        say("inj", "game exited");
    }
}

/// Follow the DLL's log until it says it finished, or `10` seconds pass.
#[cfg(windows)]
fn settle(api: &win::Api) -> Option<String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);

    while std::time::Instant::now() < deadline {
        if let Some(error) = drain(api) {
            return Some(error);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    None
}

/// Print every line the DLL logged, answering the first error it reported.
#[cfg(windows)]
fn drain(api: &win::Api) -> Option<String> {
    let mut error = None;

    while let Some(line) = api.next_log() {
        say("dll", &line);
        if line.starts_with("error:") && error.is_none() {
            error = Some(line.trim_start_matches("error:").trim().to_owned());
        }
    }

    error
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("the injection host is Windows only");
    std::process::ExitCode::FAILURE
}
