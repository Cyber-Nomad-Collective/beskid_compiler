//! Shared terminal UI helpers (Ratatui session, timers, diagnostic summaries).

pub(crate) mod diagnostics;
pub(crate) mod hyperlink;
pub(crate) mod log_input;
pub(crate) mod log_tabs;
pub(crate) mod model;
pub(crate) mod pipeline_tree;
pub(crate) mod stage_focus;
pub(crate) mod terminal;
pub(crate) mod terminal_io;
pub(crate) mod test_table;
pub(crate) mod timer;
pub(crate) mod tree;
pub(crate) mod widgets;

pub use crate::tui::shell::state::NavTarget;
pub use diagnostics::{SeverityCounts, count_severities, format_severity_summary};
pub use hyperlink::FileLineLink;
pub use model::{CommandSummary, PipelineProgress, SummarySlice, SummaryStat, TestReportSummary};
pub use pipeline_tree::PipelineTree;
pub use terminal::{PipelineViewState, TuiSession, reset_stderr_ansi};
pub use test_table::{TestRow, TestRowState, TestRunUi};
pub use timer::format_duration;
pub use tree::{format_phase_end, format_phase_start, format_work_unit};

pub fn severity_command_summary(
    title: impl Into<String>,
    headline: impl Into<String>,
    counts: SeverityCounts,
) -> CommandSummary {
    use ratatui::style::Color;

    let title = title.into();
    let headline = headline.into();
    let total = (counts.errors + counts.warnings + counts.notes).max(1) as f64;
    let mut slices = Vec::new();
    if counts.errors > 0 {
        slices.push(SummarySlice {
            label: "error".into(),
            percent: counts.errors as f64 * 100.0 / total,
            color: Color::Red,
        });
    }
    if counts.warnings > 0 {
        slices.push(SummarySlice {
            label: "warn".into(),
            percent: counts.warnings as f64 * 100.0 / total,
            color: Color::Yellow,
        });
    }
    if counts.notes > 0 {
        slices.push(SummarySlice {
            label: "note".into(),
            percent: counts.notes as f64 * 100.0 / total,
            color: Color::Blue,
        });
    }
    CommandSummary {
        title,
        headline,
        stats: vec![
            SummaryStat { label: "errors".into(), value: counts.errors.to_string(), color: Some(Color::Red) },
            SummaryStat { label: "warnings".into(), value: counts.warnings.to_string(), color: Some(Color::Yellow) },
            SummaryStat { label: "notes".into(), value: counts.notes.to_string(), color: Some(Color::Blue) },
        ],
        slices,
    }
}
