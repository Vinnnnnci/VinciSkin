use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use fs_err as fs;

use super::writer::{DailyFile, RedactingWriter, MAX_FILE_BYTES};
use super::*;

/// A writer a test can read back, since the layer takes ownership of what it writes to.
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("the sink").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// A directory of this test's own, removed when the guard drops.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);

        let path = std::env::temp_dir().join(format!(
            "vinciskin-log-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("scratch directory");

        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn day_file(&self, suffix: &str) -> PathBuf {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();

        self.0.join(format!("vinciskin.{day}{suffix}.log"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_files_the_cleanup_may_delete_are_the_ones_this_module_writes() {
    assert!(is_log_file("vinciskin.2026-11-24.log"));
    assert!(is_log_file("vinciskin.2026-11-24.1.log"));
    assert!(is_log_file("loader-20261124-233000.log"));
    assert!(is_log_file("boot.log"));
    assert!(is_log_file("boot.1.log"));
    assert!(is_log_file("panic.log"));

    assert!(!is_log_file("loader-run.cmd"));
    assert!(!is_log_file("settings.json"));
}

#[test]
fn a_day_past_the_cap_rotates_and_starts_a_second_file() {
    let scratch = Scratch::new();
    let current = scratch.day_file("");

    fs::write(&current, vec![b'x'; MAX_FILE_BYTES as usize]).expect("a file at the cap");

    let mut file = DailyFile::new(scratch.path());
    file.write_all(b"the line that tips it over\n")
        .expect("a write past the cap");
    file.flush().expect("flush");

    assert_eq!(
        fs::metadata(scratch.day_file(".1"))
            .expect("the rotated file")
            .len(),
        MAX_FILE_BYTES
    );
    assert_eq!(
        fs::read_to_string(&current).expect("the new file"),
        "the line that tips it over\n"
    );
}

#[test]
fn a_line_split_across_writes_is_redacted_once_it_is_whole() {
    const SPLIT: &str = r"C:\Tools\log-split-showcase";
    vinciskin_core::redact::register(SPLIT, "%SPLIT%");

    let scratch = Scratch::new();
    let mut writer = RedactingWriter::new(DailyFile::new(scratch.path()));

    writer
        .write_all(br"INFO reading C:\Tools\log-")
        .expect("head");
    writer
        .write_all(b"split-showcase\\Annie.fantome\n")
        .expect("tail");
    writer.flush().expect("flush");

    assert_eq!(
        fs::read_to_string(scratch.day_file("")).expect("the day's file"),
        "INFO reading %SPLIT%\\Annie.fantome\n"
    );
}

#[test]
fn the_oldest_files_go_first_when_the_directory_is_over_budget() {
    let scratch = Scratch::new();
    let start = SystemTime::now();

    let one = scratch.path().join("vinciskin.2026-11-20.log");
    let two = scratch.path().join("vinciskin.2026-11-21.log");
    let three = scratch.path().join("vinciskin.2026-11-22.log");

    for path in [&one, &two, &three] {
        fs::write(path, vec![b'x'; 6]).expect("a small file");
    }

    trim_to_budget(
        vec![
            (start - Duration::from_secs(30), 6, one.clone()),
            (start - Duration::from_secs(20), 6, two.clone()),
            (start - Duration::from_secs(10), 6, three.clone()),
        ],
        10,
    );

    assert!(!one.exists(), "the oldest file was kept");
    assert!(!two.exists(), "the second oldest file was kept");
    assert!(three.exists(), "the newest file was deleted");
}

#[test]
fn a_line_reads_as_time_level_ids_target_and_fields() {
    use tracing_subscriber::layer::SubscriberExt;

    let _ = RUN.set("c0ffee42".to_owned());

    let sink = Sink::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .event_format(super::format::LineFormat)
            .with_writer(sink.clone())
            .with_ansi(false),
    );

    tracing::subscriber::with_default(subscriber, || {
        tracing::warn!(answer = 42, "a line the file has to carry")
    });

    let written = sink.0.lock().expect("the sink").clone();
    let line = String::from_utf8(written).expect("utf-8");

    assert!(
        line.ends_with("a line the file has to carry answer=42\n"),
        "{line}"
    );
    assert!(
        line.contains(" WARN run=c0ffee42 vinciskin::logging::tests: "),
        "{line}"
    );

    let (timestamp, _) = line.split_once(' ').expect("a timestamp before the level");
    assert!(
        chrono::DateTime::parse_from_rfc3339(timestamp).is_ok(),
        "the timestamp is not RFC 3339: {timestamp}"
    );
}
