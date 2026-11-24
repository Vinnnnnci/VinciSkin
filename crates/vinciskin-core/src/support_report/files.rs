//! The individual files a report carries, each one built from what the shell collected.

use std::fmt::Write as _;

use serde::Serialize;
use serde_json::json;

use super::{LogSource, ReportFile, ReportInputs};
use crate::mods::InstalledMod;

/// Everything the logs may take together.
const LOG_BUDGET: usize = 2 * 1024 * 1024;

/// The smallest part of a log worth keeping, below which the rest is dropped instead.
const MIN_LOG_SLICE: usize = 4 * 1024;

/// How many incidents travel with a report.
const MAX_INCIDENTS: usize = 5;

/// How much of the newest log `report.md` quotes.
const TAIL_LINES: usize = 40;
const TAIL_BYTES: usize = 8 * 1024;

/// Where the report came from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentFacts {
    pub app_version: String,
    /// `release` or `debug`.
    pub channel: &'static str,
    /// `Windows 11 (26200)`, as the diagnostics name it.
    pub os: String,
    pub arch: String,
    pub webview2: Option<String>,
    pub run: Option<String>,
    pub elevated: bool,
}

/// The settings a report may carry: paths are written as placeholders, and nothing secret is here.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub league_path: Option<String>,
    pub storage_path: Option<String>,
    pub workshop_path: Option<String>,
    pub theme: String,
    pub launch_mode: String,
    pub auto_categorization: bool,
    pub promote_enabled_mods: bool,
    pub hide_riot_client_on_launch: bool,
    pub read_game_log: bool,
    pub keep_incidents: u32,
    pub minimize_to_tray: bool,
    pub start_in_tray: bool,
    pub auto_run: bool,
    pub watcher_enabled: bool,
    pub show_mod_tags: bool,
    pub verbose_patcher_logging: bool,
    pub first_run_complete: bool,
}

/// One mod as the report names it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub layers: usize,
    pub format: String,
    pub storage: String,
    pub tags: Vec<String>,
    pub champions: Vec<String>,
    pub maps: Vec<String>,
    pub installed_at: String,
}

impl From<&InstalledMod> for ModRow {
    fn from(installed: &InstalledMod) -> Self {
        Self {
            id: installed.id.clone(),
            name: installed.display_name.clone(),
            version: installed.version.clone(),
            enabled: installed.enabled,
            layers: installed.layers.len(),
            format: name_of(&installed.format),
            storage: name_of(&installed.storage),
            tags: installed.tags.clone(),
            champions: installed.champions.clone(),
            maps: installed.maps.clone(),
            installed_at: installed.installed_at.to_rfc3339(),
        }
    }
}

/// The patcher as the moment of collection found it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatcherFacts {
    pub running: bool,
    pub state: Option<String>,
    pub error: Option<String>,
    pub game_running: bool,
    /// The loader's own log, which the zip carries under `log/`.
    pub loader_log: Option<String>,
    pub game: Option<String>,
}

/// One incident, already rendered by the shell, which owns the verdict's copy.
pub struct IncidentFile {
    pub id: String,
    pub json: String,
    pub text: String,
}

/// The logs a report carries, newest first, until the budget is spent.
pub fn logs(sources: &[LogSource]) -> Vec<ReportFile> {
    let mut sources: Vec<&LogSource> = sources.iter().collect();
    sources.sort_by(|a, b| b.modified.cmp(&a.modified));

    let mut files = Vec::new();
    let mut spent = 0;

    for source in sources {
        let room = LOG_BUDGET.saturating_sub(spent);
        if room < MIN_LOG_SLICE {
            break;
        }

        let text = crate::redact::text(&source.text);
        let mut bytes = text.as_bytes().to_vec();

        if bytes.len() > room {
            bytes = tail(&bytes, room);
        }

        spent += bytes.len();
        files.push(ReportFile {
            path: format!("log/{}", file_name(&source.name)),
            bytes,
        });
    }

    files
}

/// The environment and every check the diagnostics ran.
pub fn system(inputs: &ReportInputs) -> serde_json::Value {
    json!({
        "environment": inputs.environment,
        "diagnostics": inputs.diagnostics,
    })
}

/// The last few incidents, as the record and as the text a thread reads.
pub fn incidents(list: &[IncidentFile]) -> Vec<ReportFile> {
    list.iter()
        .take(MAX_INCIDENTS)
        .flat_map(|incident| {
            let id = file_name(&incident.id);

            [
                ReportFile::text(format!("incidents/{id}.json"), &incident.json),
                ReportFile::text(format!("incidents/{id}.md"), &incident.text),
            ]
        })
        .collect()
}

/// The summary a maintainer reads first.
pub fn report_md(inputs: &ReportInputs, files: &[ReportFile]) -> String {
    let mut out = String::new();

    out.push_str("# VinciSkin support report\n\n");
    let _ = writeln!(
        out,
        "- VinciSkin: v{} ({})",
        inputs.environment.app_version, inputs.environment.channel
    );
    let _ = writeln!(
        out,
        "- System: {} {}",
        inputs.environment.os, inputs.environment.arch
    );
    let _ = writeln!(
        out,
        "- WebView2: {}",
        inputs.environment.webview2.as_deref().unwrap_or("missing")
    );
    let _ = writeln!(out, "- Problem: {}", inputs.problem.as_str());
    if let Some(run) = &inputs.environment.run {
        let _ = writeln!(out, "- Run: {run}");
    }
    out.push('\n');

    if !inputs.notes.trim().is_empty() {
        out.push_str("## What the user wrote\n\n");
        out.push_str(inputs.notes.trim());
        out.push_str("\n\n");
    }

    out.push_str("## Findings\n\n");
    for check in &inputs.diagnostics {
        let _ = writeln!(
            out,
            "- {:?} · {} · {}",
            check.severity, check.label, check.summary
        );
    }
    out.push('\n');

    out.push_str("## Files\n\n");
    for file in files {
        let _ = writeln!(out, "- {} ({})", file.path, size_label(file.bytes.len()));
    }
    let _ = writeln!(out, "- report.md (this file)");
    out.push('\n');

    if let Some(tail) = log_tail(inputs) {
        out.push_str("## Log tail\n\n```\n");
        out.push_str(&tail);
        out.push_str("```\n");
    }

    out
}

/// What a user pastes into a chat instead of attaching the zip.
pub fn summary(inputs: &ReportInputs, files: &[ReportFile]) -> String {
    let bad = inputs
        .diagnostics
        .iter()
        .filter(|check| matches!(check.severity, crate::diagnostics::Severity::Bad))
        .count();
    let warn = inputs
        .diagnostics
        .iter()
        .filter(|check| matches!(check.severity, crate::diagnostics::Severity::Warn))
        .count();
    let bytes: usize = files.iter().map(|file| file.bytes.len()).sum();

    let mut out = String::new();
    let _ = writeln!(
        out,
        "VinciSkin v{} ({}) · run {}",
        inputs.environment.app_version,
        inputs.environment.channel,
        inputs.environment.run.as_deref().unwrap_or("unknown")
    );
    let _ = writeln!(
        out,
        "{} {} · WebView2 {}",
        inputs.environment.os,
        inputs.environment.arch,
        inputs.environment.webview2.as_deref().unwrap_or("missing")
    );
    let _ = writeln!(out, "Problem: {}", inputs.problem.as_str());
    let _ = writeln!(out, "Findings: {bad} bad, {warn} warn");
    let _ = write!(out, "Report: {} files, {}", files.len(), size_label(bytes));

    out
}

/// The last lines of the newest log, which is what a reader skims first.
fn log_tail(inputs: &ReportInputs) -> Option<String> {
    let newest = inputs.logs.iter().max_by_key(|source| source.modified)?;

    let redacted = crate::redact::text(&newest.text);
    let lines: Vec<&str> = redacted.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);

    let mut tail = lines[start..].join("\n");

    /* A log can hold one 1 MB line, so the line count alone does not bound what the summary
    quotes into every copy of the report. */
    if tail.len() > TAIL_BYTES {
        let cut = tail.len() - TAIL_BYTES;
        let boundary = tail
            .char_indices()
            .find(|(offset, _)| *offset >= cut)
            .map_or(tail.len(), |(offset, _)| offset);

        tail.drain(..boundary);
        tail.insert_str(0, "[earlier lines omitted]\n");
    }

    Some(tail)
}

/// The last `room` bytes of `bytes`, moved forward to a line start.
fn tail(bytes: &[u8], room: usize) -> Vec<u8> {
    let start = bytes.len().saturating_sub(room);
    let candidate = &bytes[start..];

    let offset = candidate
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(0, |offset| offset + 1);

    candidate[offset..].to_vec()
}

/// A name safe to use inside the zip, whichever value it came from.
fn file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            other => other,
        })
        .collect();

    let cleaned = cleaned.trim_matches('.').trim();

    if cleaned.is_empty() {
        "log".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// The protocol name of a serde enum that serialises to a bare word.
fn name_of<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_default()
}

/// `1234` as a size a person reads at a glance.
pub fn size_label(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
