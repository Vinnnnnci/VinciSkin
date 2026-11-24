//! OS-level diagnostic checks: Windows version, long-paths registry,
//! UAC enabled. These don't depend on any user paths and run on app launch.

use super::{Category, Check, Severity, check};

#[cfg(target_os = "windows")]
use super::win_util::{HKCU, HKLM, reg_read_num, reg_read_str};
#[cfg(target_os = "windows")]
use super::{CheckDetail, check_ok};

#[cfg(target_os = "windows")]
const MIN_OK_BUILD: u32 = 19045;
#[cfg(target_os = "windows")]
const KNOWN_BAD_BUILD: u32 = 22000;

/// The build at which Windows reports itself as Windows 11.
const WINDOWS_11_BUILD: u32 = 22000;

/// The Windows name a build number belongs to.
#[must_use]
pub fn windows_name(build: u32) -> &'static str {
    if build >= WINDOWS_11_BUILD {
        "Windows 11"
    } else {
        "Windows 10"
    }
}

/// The system as a person reads it, `Windows 11 (build 26200)`.
#[must_use]
pub fn describe_os() -> String {
    #[cfg(target_os = "windows")]
    {
        let (_, _, build) = read_kuser_version();

        format!("{} (build {build})", windows_name(build))
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::consts::OS.to_owned()
    }
}

/// The oldest WebView2 major the frontend's build target runs on.
const MIN_WEBVIEW2_MAJOR: u32 = 105;

/// Whether `version` is older than the frontend's build target.
///
/// A version that does not parse is answered `false`: a runtime that reported something is not
/// the failure a missing one is.
#[must_use]
pub fn webview2_too_old(version: &str) -> bool {
    version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_some_and(|major| major < MIN_WEBVIEW2_MAJOR)
}

/// Read OS major/minor/build from the kernel-shared user data page (KUSER_SHARED_DATA).
/// This avoids `GetVersionExW` lying when the manager isn't manifested for the current OS.
#[cfg(target_os = "windows")]
fn read_kuser_version() -> (u32, u32, u32) {
    // SAFETY: KUSER_SHARED_DATA is mapped read-only at 0x7FFE0000 on every
    // Windows process; reads from these documented offsets are well-defined.
    unsafe {
        let major = std::ptr::read_volatile((0x7ffe0000 + 0x26c) as *const u32);
        let minor = std::ptr::read_volatile((0x7ffe0000 + 0x270) as *const u32);
        let build = std::ptr::read_volatile((0x7ffe0000 + 0x260) as *const u32);
        (major, minor, build)
    }
}

#[cfg(target_os = "windows")]
pub fn check_version() -> Check {
    let (major, minor, build) = read_kuser_version();
    let display = format!("{} (build {build})", windows_name(build));
    if build < MIN_OK_BUILD || build == KNOWN_BAD_BUILD {
        let mut c = check(
            "windows.version",
            "Windows version",
            Category::System,
            Severity::Bad,
            display,
        );
        c.suggestion = Some(
            "Your Windows build is older than the minimum supported by League. Run Windows Update to install the latest cumulative update before troubleshooting further."
                .into(),
        );
        c.details.push(CheckDetail::new("major", major.to_string()));
        c.details.push(CheckDetail::new("minor", minor.to_string()));
        c.details.push(CheckDetail::new("build", build.to_string()));
        c
    } else {
        let mut c = check_ok(
            "windows.version",
            "Windows version",
            Category::System,
            &display,
        );
        c.details.push(CheckDetail::new("build", build.to_string()));
        c
    }
}

#[cfg(not(target_os = "windows"))]
pub fn check_version() -> Check {
    check(
        "windows.version",
        "Windows version",
        Category::System,
        Severity::Info,
        "Not running on Windows",
    )
}

#[cfg(target_os = "windows")]
pub fn check_long_paths_enabled() -> Check {
    let value = reg_read_num(
        HKLM,
        "SYSTEM\\CurrentControlSet\\Control\\FileSystem",
        "LongPathsEnabled",
    )
    .unwrap_or(0);
    if value == 0 {
        let mut c = check(
            "windows.long_paths",
            "Long paths enabled",
            Category::System,
            Severity::Warn,
            "Disabled — paths longer than 260 chars will fail",
        );
        c.suggestion = Some(
            "Mods stored deep in folders (especially under OneDrive) can hit the legacy 260-char path limit. Enable long-path support in the Windows registry."
                .into(),
        );
        c.fix_command = Some(
            r#"reg add "HKLM\SYSTEM\CurrentControlSet\Control\FileSystem" /v LongPathsEnabled /t REG_DWORD /d 1 /f"#
                .into(),
        );
        c
    } else {
        check_ok(
            "windows.long_paths",
            "Long paths enabled",
            Category::System,
            "Enabled",
        )
    }
}

#[cfg(not(target_os = "windows"))]
pub fn check_long_paths_enabled() -> Check {
    check(
        "windows.long_paths",
        "Long paths enabled",
        Category::System,
        Severity::Info,
        "Not applicable",
    )
}

#[cfg(target_os = "windows")]
pub fn check_uac_enabled() -> Check {
    let value = reg_read_num(
        HKLM,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System",
        "EnableLUA",
    )
    .unwrap_or(1);
    if value == 0 {
        let mut c = check(
            "windows.uac",
            "User Account Control",
            Category::System,
            Severity::Bad,
            "Disabled — every process runs elevated",
        );
        c.suggestion = Some(
            "UAC is disabled, which means everything runs as administrator. League's anti-cheat and the patcher both rely on UAC being on. Re-enable it under User Accounts → Change User Account Control settings, then reboot."
                .into(),
        );
        c
    } else {
        check_ok(
            "windows.uac",
            "User Account Control",
            Category::System,
            "Enabled",
        )
    }
}

#[cfg(not(target_os = "windows"))]
pub fn check_uac_enabled() -> Check {
    check(
        "windows.uac",
        "User Account Control",
        Category::System,
        Severity::Info,
        "Not applicable",
    )
}

/// The Evergreen WebView2 runtime's version, from the registry key either install writes.
///
/// The machine-wide key is the 32-bit view, because that is where the runtime's own installer
/// writes it. The per-user key is the fallback, for a runtime a user installed without admin.
#[cfg(target_os = "windows")]
pub fn webview2_version() -> Option<String> {
    const MACHINE_KEY: &str =
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    const USER_KEY: &str =
        r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    reg_read_str(HKLM, MACHINE_KEY, "pv").or_else(|| reg_read_str(HKCU, USER_KEY, "pv"))
}

#[cfg(not(target_os = "windows"))]
pub fn webview2_version() -> Option<String> {
    None
}

/// The WebView2 runtime, which this app cannot draw a window without.
#[cfg(target_os = "windows")]
pub fn check_webview2() -> Check {
    let Some(version) = webview2_version() else {
        let mut c = check(
            "windows.webview2",
            "WebView2 runtime",
            Category::System,
            Severity::Bad,
            "Missing — the window cannot be drawn",
        );
        c.suggestion = Some(
            "VinciSkin draws its window with the WebView2 runtime, which Windows 11 ships and Windows 10 often does not. Install the Evergreen runtime from Microsoft's page, then start VinciSkin again."
                .into(),
        );
        return c;
    };

    let major = version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .unwrap_or(0);

    if webview2_too_old(&version) {
        let mut c = check(
            "windows.webview2",
            "WebView2 runtime",
            Category::System,
            Severity::Warn,
            format!("{version} — older than the app's build target"),
        );
        c.suggestion = Some(
            "The runtime is older than the Chromium build VinciSkin's frontend targets, so pages may fail to render. Windows Update or the Evergreen installer brings it up to date."
                .into(),
        );
        c
    } else {
        let mut c = check_ok(
            "windows.webview2",
            "WebView2 runtime",
            Category::System,
            &version,
        );
        c.details.push(CheckDetail::new("build", major.to_string()));
        c
    }
}

#[cfg(not(target_os = "windows"))]
pub fn check_webview2() -> Check {
    check(
        "windows.webview2",
        "WebView2 runtime",
        Category::System,
        Severity::Info,
        "Not applicable",
    )
}

/// Whether this build of the app is 64-bit, which the injection loader requires.
pub fn check_arch() -> Check {
    let arch = std::env::consts::ARCH;

    if arch == "x86" {
        let mut c = check(
            "system.arch",
            "Architecture",
            Category::System,
            Severity::Bad,
            "32-bit — the injection loader is 64-bit only",
        );
        c.suggestion = Some(
            "Install the 64-bit build of VinciSkin. League, the loader and the overlay all run in one 64-bit process, which a 32-bit manager cannot reach."
                .into(),
        );
        return c;
    }

    check_ok("system.arch", "Architecture", Category::System, arch)
}

/// The .NET Framework release number at which 4.8 starts.
#[cfg(target_os = "windows")]
const NET48_RELEASE: u64 = 528_040;

/// The .NET Framework the Chinese client's anti-cheat component loads.
#[cfg(target_os = "windows")]
pub fn check_dotnet_framework() -> Check {
    const KEY: &str = "SOFTWARE\\Microsoft\\NET Framework Setup\\NDP\\v4\\Full";

    let release = reg_read_num(HKLM, KEY, "Release")
        .or_else(|| reg_read_num(HKLM, &format!("SOFTWARE\\WOW6432Node\\{KEY}"), "Release"));

    match release {
        Some(release) if release >= NET48_RELEASE => check_ok(
            "system.dotnet_framework",
            ".NET Framework",
            Category::System,
            "4.8",
        ),
        Some(release) => {
            let mut c = check(
                "system.dotnet_framework",
                ".NET Framework",
                Category::System,
                Severity::Warn,
                format!("4.x (release {release}) — older than 4.8"),
            );
            c.suggestion = Some(
                "The Chinese client starts League through a component that needs .NET Framework 4.8. Windows Update offers it on Windows 10 1903 and later."
                    .into(),
            );
            c
        }
        None => {
            let mut c = check(
                "system.dotnet_framework",
                ".NET Framework",
                Category::System,
                Severity::Warn,
                "Not installed",
            );
            c.suggestion = Some(
                "The Chinese client starts League through a component that needs .NET Framework 4.8. Windows Update offers it on Windows 10 1903 and later."
                    .into(),
            );
            c
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn check_dotnet_framework() -> Check {
    check(
        "system.dotnet_framework",
        ".NET Framework",
        Category::System,
        Severity::Info,
        "Not applicable",
    )
}

#[cfg(test)]
mod tests;
