//! The support report: one zip a user sends instead of being asked for four files.
//!
//! Everything here is built from facts the app already holds. Nothing walks a directory, so a
//! report cannot grow a file nobody chose, and every text is passed through
//! [`redact`](crate::redact) before it is written - including what the user typed, which is the
//! one place an account name is most likely to appear.

use std::io::Write;
use std::path::Path;

use serde_json::json;

mod files;

#[cfg(test)]
mod tests;

pub use files::{EnvironmentFacts, IncidentFile, ModRow, PatcherFacts, SettingsSnapshot};

/// The version of the layout this module writes, carried so a reader can tell two apart.
const SCHEMA: u32 = 1;

/// A file a report carries, as its path inside the zip and its bytes.
pub struct ReportFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

impl ReportFile {
    /// A text file, redacted on the way in.
    pub(crate) fn text(path: impl Into<String>, text: &str) -> Self {
        Self {
            path: path.into(),
            bytes: crate::redact::text(text).as_bytes().to_vec(),
        }
    }

    /// A JSON file, serialised then redacted, since a path sits inside it as text.
    pub(crate) fn json<T: serde::Serialize>(path: impl Into<String>, value: &T) -> Self {
        let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_owned());

        Self::text(path, &text)
    }
}

/// A log the app already wrote, offered to the report.
pub struct LogSource {
    /// The file's own name, which the report keeps.
    pub name: String,
    pub text: String,
    /// When it was last written, which decides which logs the budget buys.
    pub modified: Option<std::time::SystemTime>,
}

/// What a report is built from, collected by the shell.
pub struct ReportInputs {
    pub environment: EnvironmentFacts,
    pub diagnostics: Vec<crate::diagnostics::Check>,
    pub settings: SettingsSnapshot,
    pub library: Vec<ModRow>,
    pub patcher: PatcherFacts,
    pub incidents: Vec<IncidentFile>,
    pub logs: Vec<LogSource>,
    /// The game's own log, carried only when the user asked for it.
    pub game_logs: Vec<LogSource>,
    pub problem: ProblemKind,
    /// What the user wrote about the problem, free of charge and of redaction until now.
    pub notes: String,
}

/// Why the user is sending this, which decides what the report leads with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    ModNotShowing,
    GameCrash,
    AppBroken,
    Other,
}

impl ProblemKind {
    /// The word `report.md` and `manifest.json` carry.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ModNotShowing => "mod-not-showing",
            Self::GameCrash => "game-crash",
            Self::AppBroken => "app-broken",
            Self::Other => "other",
        }
    }

    /// The kind `word` names, answered as [`Other`](Self::Other) when it names none.
    #[must_use]
    pub fn parse(word: &str) -> Self {
        match word {
            "mod-not-showing" => Self::ModNotShowing,
            "game-crash" => Self::GameCrash,
            "app-broken" => Self::AppBroken,
            _ => Self::Other,
        }
    }
}

/// The built report.
pub struct Report {
    files: Vec<ReportFile>,
    summary: String,
}

impl Report {
    /// The files this report carries, in the order the zip writes them.
    #[must_use]
    pub fn files(&self) -> &[ReportFile] {
        &self.files
    }

    /// The whole report's size, before compression.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.files.iter().map(|file| file.bytes.len()).sum()
    }

    /// The short text a user pastes into a chat, which names the zip and what it holds.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// The name a saved zip takes, carrying the run so a maintainer can match it to a log.
    #[must_use]
    pub fn file_name(&self, run: Option<&str>) -> String {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let run = run.unwrap_or("norun");

        format!("VinciSkin-report-{stamp}-{run}.zip")
    }

    /// Write the report to `path` as a zip.
    pub fn write_zip(&self, path: &Path) -> Result<(), ReportError> {
        let file = fs_err::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        for entry in &self.files {
            zip.start_file(entry.path.as_str(), options)?;
            zip.write_all(&entry.bytes)?;
        }

        zip.finish()?;

        Ok(())
    }
}

/// Everything that can stop a report from being built or saved.
#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("the report could not be written: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the report could not be written: {0}")]
    Io(#[from] std::io::Error),
}

/// Build every file the report carries, in the order the preview lists them.
pub fn build(inputs: &ReportInputs) -> Result<Report, ReportError> {
    let mut files = files::logs("log", &inputs.logs);
    files.extend(files::logs("game-log", &inputs.game_logs));

    files.push(ReportFile::json("system.json", &files::system(inputs)));
    files.push(ReportFile::json("settings.json", &inputs.settings));
    files.push(ReportFile::json("library.json", &inputs.library));
    files.push(ReportFile::json("patcher.json", &inputs.patcher));
    files.extend(files::incidents(&inputs.incidents));

    let report = files::report_md(inputs, &files);
    files.insert(0, ReportFile::text("report.md", &report));

    let summary = files::summary(inputs, &files);

    let manifest = json!({
        "schema": SCHEMA,
        "generatedAt": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        "app": {
            "version": inputs.environment.app_version,
            "channel": inputs.environment.channel,
            "run": inputs.environment.run,
        },
        "problem": inputs.problem.as_str(),
        "redaction": crate::redact::counts(),
        "files": files
            .iter()
            .map(|file| json!({ "path": file.path, "bytes": file.bytes.len() }))
            .collect::<Vec<_>>(),
    });

    files.push(ReportFile::json("manifest.json", &manifest));

    Ok(Report { files, summary })
}
