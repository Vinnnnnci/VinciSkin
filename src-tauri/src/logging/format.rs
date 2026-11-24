//! The line a log file record is written as: local time, level, the three ids, target, fields.
//!
//! The ids cannot ride on a span. This app logs from a command thread, the patcher thread, the
//! overlay builder and library scans, and a span only reaches the thread that entered it.

use std::fmt;

use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::{format::Writer, FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// The file format, which names the run, session and game a line belongs to.
pub struct LineFormat;

impl<S, N> FormatEvent<S, N> for LineFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();

        write!(writer, "{} {:>5}", super::now(), metadata.level())?;

        if let Some(run) = super::run_id() {
            write!(writer, " run={run}")?;
        }
        if let Some(session) = super::session_id() {
            write!(writer, " session={session}")?;
        }
        if let Some(game) = super::game_id() {
            write!(writer, " game={game}")?;
        }

        write!(writer, " {}: ", metadata.target())?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}
