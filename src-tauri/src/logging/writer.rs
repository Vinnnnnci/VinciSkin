//! Where a log line lands: the day's file, redacted on the way and rotated at 5 MB.
//!
//! Both steps sit behind the writer rather than at the call sites, because a call site is the
//! one place a private value is certain to slip past. The appender this replaces sizes a file by
//! the day alone, which a trace-level session can exhaust long before midnight.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

use fs_err as fs;

/// The size a day's log is rotated at.
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// A writer that replaces private values before a line reaches the file it wraps.
///
/// A line arrives in as many writes as `fmt` chooses, so the tail of an incomplete one is held
/// until its newline arrives. The held bytes stay undecoded, because a write can split a
/// multi-byte character as readily as a field.
pub struct RedactingWriter<W> {
    inner: W,
    pending: Vec<u8>,
}

impl<W> RedactingWriter<W> {
    /// Wrap `inner`, redacting every line written through it.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
        }
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);

        let mut written = 0;
        while let Some(end) = self.pending[written..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| written + offset + 1)
        {
            let line = String::from_utf8_lossy(&self.pending[written..end]).into_owned();
            self.inner
                .write_all(vinciskin_core::redact::text(&line).as_bytes())?;
            written = end;
        }
        self.pending.drain(..written);

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            let line = String::from_utf8_lossy(&self.pending).into_owned();
            self.inner
                .write_all(vinciskin_core::redact::text(&line).as_bytes())?;
            self.pending.clear();
        }

        self.inner.flush()
    }
}

/// The day's file, reopened when the date turns over and rotated when it grows past the cap.
pub struct DailyFile {
    dir: PathBuf,
    day: Option<String>,
    file: Option<File>,
    written: u64,
}

impl DailyFile {
    /// Answer a writer for `dir`, opening the day's file on the first line.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            day: None,
            file: None,
            written: 0,
        }
    }

    /// Open the day's file, appending to what an earlier run left.
    fn open(&mut self) -> io::Result<()> {
        let day = today();
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(&day))?;

        self.written = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        self.file = Some(file);
        self.day = Some(day);

        Ok(())
    }

    /// Rename the day's file aside and start a second one.
    fn rotate(&mut self) -> io::Result<()> {
        let Some(day) = self.day.clone() else {
            return self.open();
        };

        self.file = None;

        let current = self.path(&day);
        let rotated = self.dir.join(format!("vinciskin.{day}.1.log"));
        let _ = fs::remove_file(&rotated);

        /* The writer is the last place that may report a failure: tracing would come back
        here, so what the user sees is the console line the boot timeline also writes. */
        if let Err(error) = fs::rename(&current, &rotated) {
            eprintln!("failed to rotate {}: {error}", current.display());
        }

        self.open()
    }

    fn path(&self, day: &str) -> PathBuf {
        self.dir.join(format!("vinciskin.{day}.log"))
    }
}

impl Write for DailyFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let day = today();

        if self.file.is_none() || self.day.as_deref() != Some(day.as_str()) {
            self.open()?;
        }

        /* The check follows the open rather than sitting in its `else`: a file an earlier run
        left at the cap has to rotate before the first line lands in it. */
        if self.written > 0 && self.written + buf.len() as u64 > MAX_FILE_BYTES {
            self.rotate()?;
        }

        let Some(file) = self.file.as_mut() else {
            return Err(io::Error::other("the log file is not open"));
        };

        file.write_all(buf)?;
        self.written += buf.len() as u64;

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(file) => file.flush(),
            None => Ok(()),
        }
    }
}

/// The local date a log file is named after.
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
