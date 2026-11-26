use std::io::Write;

use super::*;

#[test]
fn a_loader_script_quotes_every_path() {
    let script = loader_script(
        Path::new(r"C:\Program Files\VinciSkin\vinciskin-inj.exe"),
        Path::new(r"C:\Program Files\VinciSkin\cslol-dll-legacy.dll"),
        Path::new(r"D:\Mod Storage\overlay"),
        Path::new(r"C:\Users\A B\AppData\Roaming\com.vinciskin.app\logs\loader-1.log"),
    );

    assert!(script.contains(
        r#""C:\Program Files\VinciSkin\vinciskin-inj.exe" --dll "C:\Program Files\VinciSkin\cslol-dll-legacy.dll" --overlay "D:\Mod Storage\overlay" > "C:\Users\A B\AppData\Roaming\com.vinciskin.app\logs\loader-1.log" 2>&1"#
    ));
}

#[test]
fn a_percent_in_a_path_is_doubled_for_cmd() {
    let script = loader_script(
        Path::new(r"D:\100%\vinciskin-inj.exe"),
        Path::new(r"D:\100%\cslol-dll-legacy.dll"),
        Path::new(r"D:\100%\overlay"),
        Path::new(r"D:\100%\loader.log"),
    );

    assert!(script.contains(r#""D:\100%%\vinciskin-inj.exe""#));
    assert!(script.contains(r#"--dll "D:\100%%\cslol-dll-legacy.dll""#));
    assert!(script.contains(r#"--overlay "D:\100%%\overlay""#));
}

#[test]
fn an_acknowledged_dll_is_injected() {
    let mut inner = Inner::default();

    apply_loader_line(&mut inner, "[inj] dll acknowledged in 812 ms");

    assert_eq!(inner.status.state.as_deref(), Some("injected"));
    assert!(inner.status.error.is_none());
}

#[test]
fn a_target_process_is_attached_but_not_injected() {
    let mut inner = Inner::default();

    apply_loader_line(&mut inner, r#"[inj] game found pid=4242 tid=99"#);
    assert_eq!(inner.status.state.as_deref(), Some("attached"));

    apply_loader_line(&mut inner, "[dll] info: Init done!");
    assert_eq!(inner.status.state.as_deref(), Some("injected"));
}

#[test]
fn a_host_failure_is_the_status_error() {
    let mut inner = Inner::default();

    apply_loader_line(
        &mut inner,
        "[inj] failed: no game window answered in 30 minutes",
    );

    assert_eq!(inner.status.state.as_deref(), Some("failed"));
    assert_eq!(
        inner.status.error.as_deref(),
        Some("no game window answered in 30 minutes")
    );
}

#[test]
fn a_dll_patch_failure_is_the_status_error() {
    let mut inner = Inner::default();

    apply_loader_line(&mut inner, "[dll] error: Failed to patch CreateFileA!");

    assert_eq!(inner.status.state.as_deref(), Some("failed"));
    assert_eq!(
        inner.status.error.as_deref(),
        Some("Failed to patch CreateFileA!")
    );
}

#[test]
fn a_progress_line_is_not_a_failure() {
    let mut inner = Inner::default();

    apply_loader_line(&mut inner, "[inj] waiting for game");
    apply_loader_line(&mut inner, "[dll] info: Init in process!");
    apply_loader_line(
        &mut inner,
        r#"[LoaderDiag] OpenThread(THREAD_QUERY_LIMITED_INFORMATION) failed; Win32=87"#,
    );

    assert!(inner.status.state.is_none());
    assert!(inner.status.error.is_none());
}

#[test]
fn refresh_reads_only_lines_it_has_not_seen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loader.log");
    fs::write(&path, "[inj] ready\n").unwrap();

    let mut inner = Inner {
        log_path: Some(path.clone()),
        ..Inner::default()
    };
    refresh_from_log(&mut inner);
    assert!(inner.status.state.is_none());

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "[inj] failed: Win32=5").unwrap();
    drop(file);

    refresh_from_log(&mut inner);
    assert_eq!(inner.status.error.as_deref(), Some("Win32=5"));
    assert_eq!(inner.status.state.as_deref(), Some("failed"));
}

#[test]
fn a_partial_line_waits_for_its_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loader.log");
    fs::write(&path, "[inj] failed: Win32").unwrap();

    let mut inner = Inner {
        log_path: Some(path.clone()),
        ..Inner::default()
    };
    refresh_from_log(&mut inner);
    assert!(inner.status.error.is_none());

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "=5").unwrap();
    drop(file);

    refresh_from_log(&mut inner);
    assert_eq!(inner.status.error.as_deref(), Some("Win32=5"));
}
