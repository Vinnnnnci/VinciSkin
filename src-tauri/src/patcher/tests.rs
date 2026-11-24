use std::io::Write;

use super::*;

#[test]
fn a_loader_script_quotes_every_path() {
    let script = loader_script(
        Path::new(r"C:\Program Files\VinciSkin\cslol-inj.exe"),
        Path::new(r"D:\Mod Storage\overlay"),
        Path::new(r"C:\Users\A B\AppData\Roaming\com.vinciskin.app\logs\loader-1.log"),
    );

    assert!(script.contains(
        r#""C:\Program Files\VinciSkin\cslol-inj.exe" "D:\Mod Storage\overlay" > "C:\Users\A B\AppData\Roaming\com.vinciskin.app\logs\loader-1.log" 2>&1"#
    ));
}

#[test]
fn a_percent_in_a_path_is_doubled_for_cmd() {
    let script = loader_script(
        Path::new(r"D:\100%\cslol-inj.exe"),
        Path::new(r"D:\100%\overlay"),
        Path::new(r"D:\100%\loader.log"),
    );

    assert!(script.contains(r#""D:\100%%\cslol-inj.exe""#));
    assert!(script.contains(r#""D:\100%%\overlay""#));
}

#[test]
fn an_acknowledged_dll_is_injected() {
    let mut inner = Inner::default();

    apply_loader_line(
        &mut inner,
        "[LoaderDiag] DLL initialization entry acknowledged; see DLL log for initialization outcome.",
    );

    assert_eq!(inner.status.state.as_deref(), Some("injected"));
    assert!(inner.status.error.is_none());
}

#[test]
fn a_target_process_is_attached_but_not_injected() {
    let mut inner = Inner::default();

    apply_loader_line(&mut inner, "[DIAG] target pid=4242");
    assert_eq!(inner.status.state.as_deref(), Some("attached"));

    apply_loader_line(&mut inner, "Init done! waiting for the game to exit");
    assert_eq!(inner.status.state.as_deref(), Some("injected"));
}

#[test]
fn a_hook_failure_is_the_status_error() {
    let mut inner = Inner::default();

    apply_loader_line(&mut inner, "Failed to hook: Win32=5");
    assert_eq!(inner.status.state.as_deref(), Some("failed"));
    assert_eq!(
        inner.status.error.as_deref(),
        Some("Failed to hook: Win32=5")
    );
}

#[test]
fn a_prepared_line_is_not_a_failure() {
    let mut inner = Inner::default();

    apply_loader_line(
        &mut inner,
        "[LoaderDiag] Prepared; waiting for verified profile.",
    );

    assert!(inner.status.state.is_none());
    assert!(inner.status.error.is_none());
}

#[test]
fn refresh_reads_only_lines_it_has_not_seen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loader.log");
    fs::write(
        &path,
        "[LoaderDiag] Prepared; waiting for verified profile.\n",
    )
    .unwrap();

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
    writeln!(file, "Failed to hook: Win32=5").unwrap();
    drop(file);

    refresh_from_log(&mut inner);
    assert_eq!(
        inner.status.error.as_deref(),
        Some("Failed to hook: Win32=5")
    );
    assert_eq!(inner.status.state.as_deref(), Some("failed"));
}

#[test]
fn a_partial_line_waits_for_its_newline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loader.log");
    fs::write(&path, "Failed to hook").unwrap();

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
    writeln!(file, ": Win32=5").unwrap();
    drop(file);

    refresh_from_log(&mut inner);
    assert_eq!(
        inner.status.error.as_deref(),
        Some("Failed to hook: Win32=5")
    );
}
