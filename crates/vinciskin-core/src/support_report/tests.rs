use std::io::Read;
use std::time::{Duration, SystemTime};

use super::*;

/// Inputs that carry the two things a report must never leak: a profile path and an address.
fn inputs(logs: Vec<LogSource>) -> ReportInputs {
    ReportInputs {
        environment: EnvironmentFacts {
            app_version: "1.23.0".to_owned(),
            channel: "release",
            os: "Windows 11 (26200)".to_owned(),
            arch: "x86_64".to_owned(),
            webview2: Some("158.0.4370.33".to_owned()),
            run: Some("4be1c07a".to_owned()),
            elevated: false,
        },
        diagnostics: Vec::new(),
        settings: SettingsSnapshot::default(),
        library: Vec::new(),
        patcher: PatcherFacts::default(),
        incidents: Vec::new(),
        logs,
        game_logs: Vec::new(),
        problem: ProblemKind::Other,
        notes: String::new(),
    }
}

/// One log, as the shell reads it off disk.
fn log(name: &str, text: &str, age: Duration) -> LogSource {
    LogSource {
        name: name.to_owned(),
        text: text.to_owned(),
        modified: Some(SystemTime::now() - age),
    }
}

/// The report's files, as path and text, for an assertion that reads them.
fn texts(report: &Report) -> Vec<(String, String)> {
    report
        .files()
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                String::from_utf8_lossy(&file.bytes).into_owned(),
            )
        })
        .collect()
}

#[test]
fn a_report_carries_the_summary_and_one_file_per_fact() {
    let mut inputs = inputs(vec![log(
        "vinciskin.2026-11-25.log",
        "INFO hello\n",
        Duration::ZERO,
    )]);
    inputs.problem = ProblemKind::GameCrash;

    let report = build(&inputs).expect("a report");
    let paths: Vec<String> = report
        .files()
        .iter()
        .map(|file| file.path.clone())
        .collect();

    assert_eq!(
        paths,
        [
            "report.md",
            "log/vinciskin.2026-11-25.log",
            "system.json",
            "settings.json",
            "library.json",
            "patcher.json",
            "manifest.json",
        ]
    );
    assert!(report.summary().contains("VinciSkin v1.23.0"));
    assert!(report.summary().contains("game-crash"));
}

#[test]
fn no_file_carries_a_profile_path_or_an_address() {
    let mut inputs = inputs(vec![log(
        "vinciskin.2026-11-25.log",
        "opened C:\\Users\\canary-account\\Documents\\mods and mailed canary@example.com\n",
        Duration::ZERO,
    )]);
    inputs.notes = "it fails under C:/Users/canary-account/AppData".to_owned();

    let report = build(&inputs).expect("a report");

    for (path, text) in texts(&report) {
        assert!(!text.contains("canary-account"), "{path} names the account");
        assert!(
            !text.contains("canary@example.com"),
            "{path} carries the address"
        );
    }
}

#[test]
fn the_logs_budget_buys_the_newest_files_first() {
    let big = "x".repeat(1024 * 1024);

    let report = build(&inputs(vec![
        log("vinciskin.2026-11-24.log", &big, Duration::from_secs(3600)),
        log("vinciskin.2026-11-23.log", &big, Duration::from_secs(7200)),
        log("vinciskin.2026-11-22.log", &big, Duration::from_secs(10800)),
    ]))
    .expect("a report");

    let logs: Vec<&ReportFile> = report
        .files()
        .iter()
        .filter(|file| file.path.starts_with("log/"))
        .collect();

    assert_eq!(logs.len(), 2, "the budget should buy the two newest");
    assert_eq!(logs[0].path, "log/vinciskin.2026-11-24.log");
    assert!(report.total_bytes() <= 3 * 1024 * 1024);
}

#[test]
fn the_manifest_names_every_file_it_sits_beside() {
    let report = build(&inputs(vec![log(
        "boot.log",
        "stage=app-start\n",
        Duration::ZERO,
    )]))
    .expect("a report");

    let manifest = texts(&report)
        .into_iter()
        .find(|(path, _)| path == "manifest.json")
        .expect("a manifest")
        .1;

    let value: serde_json::Value = serde_json::from_str(&manifest).expect("valid json");
    let listed = value["files"].as_array().expect("a list of files");

    assert_eq!(listed.len(), report.files().len() - 1);
    assert_eq!(value["schema"], 1);
    assert!(
        listed
            .iter()
            .any(|entry| entry["path"] == "report.md" && entry["bytes"].as_u64().is_some())
    );
}

#[test]
fn an_incident_travels_both_as_its_record_and_its_text() {
    let mut inputs = inputs(Vec::new());
    inputs.incidents = vec![IncidentFile {
        id: "2026-11-25T00-30-02".to_owned(),
        json: r#"{"id":"2026-11-25T00-30-02"}"#.to_owned(),
        text: "# VinciSkin - League diagnostics\n".to_owned(),
    }];

    let paths: Vec<String> = build(&inputs)
        .expect("a report")
        .files()
        .iter()
        .map(|file| file.path.clone())
        .collect();

    assert!(paths.contains(&"incidents/2026-11-25T00-30-02.json".to_owned()));
    assert!(paths.contains(&"incidents/2026-11-25T00-30-02.md".to_owned()));
}

#[test]
fn the_zip_holds_every_file_the_report_built() {
    let report = build(&inputs(vec![log(
        "vinciskin.2026-11-25.log",
        "INFO hello\n",
        Duration::ZERO,
    )]))
    .expect("a report");

    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join(report.file_name(Some("4be1c07a")));
    report.write_zip(&path).expect("a written zip");

    assert!(
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("VinciSkin-report-") && name.ends_with("-4be1c07a.zip")
            })
    );

    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&path).expect("the zip")).expect("a readable zip");

    assert_eq!(archive.len(), report.files().len());

    for file in report.files() {
        let mut entry = archive.by_name(&file.path).expect("every file in the zip");
        let mut text = Vec::new();
        entry.read_to_end(&mut text).expect("the entry");

        assert_eq!(text, file.bytes, "{} differs", file.path);
    }
}

#[test]
fn a_name_that_would_leave_the_directory_is_flattened() {
    let report = build(&inputs(vec![log(
        "../vinciskin log?.txt",
        "x\n",
        Duration::ZERO,
    )]))
    .expect("a report");

    assert!(
        report
            .files()
            .iter()
            .all(|file| !file.path.contains("..") && !file.path.contains('?')),
        "{:?}",
        report
            .files()
            .iter()
            .map(|file| &file.path)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_game_log_travels_under_its_own_directory_only_when_offered() {
    let mut inputs = inputs(vec![log("vinciskin.2026-11-25.log", "x\n", Duration::ZERO)]);

    let without = build(&inputs).expect("a report");
    assert!(
        without
            .files()
            .iter()
            .all(|file| !file.path.starts_with("game-log/"))
    );

    inputs.game_logs = vec![log("r3dlog.txt", "saw PUUID(<account>)\n", Duration::ZERO)];
    let with = build(&inputs).expect("a report");

    let game: Vec<&ReportFile> = with
        .files()
        .iter()
        .filter(|file| file.path.starts_with("game-log/"))
        .collect();

    assert_eq!(game.len(), 1);
    assert_eq!(game[0].path, "game-log/r3dlog.txt");
}
