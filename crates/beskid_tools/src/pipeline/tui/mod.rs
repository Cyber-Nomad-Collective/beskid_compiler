//! Line-oriented pipeline output helpers: diagnostics, timers, trees, and summaries.

pub(crate) mod diagnostics;
pub(crate) mod hyperlink;
pub(crate) mod model;
pub(crate) mod test_table;
pub(crate) mod timer;
pub(crate) mod tree;

pub use diagnostics::{SeverityCounts, count_severities, format_severity_summary};
pub use hyperlink::FileLineLink;
pub use model::{CommandSummary, SummaryStat};
pub use test_table::{TestRow, TestRowState, TestRunUi};
pub use timer::format_duration;
pub use tree::{format_phase_end, format_phase_start, format_work_unit};

pub fn severity_command_summary(
    title: impl Into<String>,
    headline: impl Into<String>,
    counts: SeverityCounts,
) -> CommandSummary {
    CommandSummary::plain(title, headline)
        .with_stat("errors", counts.errors.to_string())
        .with_stat("warnings", counts.warnings.to_string())
        .with_stat("notes", counts.notes.to_string())
}
