//! Shared terminal UI helpers (Ratatui TEA session, timers, diagnostic summaries).

mod diagnostics;
mod hyperlink;
mod layout;
mod message;
mod model;
mod pipeline_tree;
mod terminal;
mod test_report;
mod test_table;
mod timer;
mod tree;
mod update;
mod view;
mod widgets;

pub use diagnostics::{SeverityCounts, count_severities, format_severity_summary};
pub use hyperlink::{FileLineLink, hyperlinks_enabled};
pub use model::{
    CommandSummary, PipelineProgress, SummarySlice, SummaryStat, TestReportSummary,
};
pub use pipeline_tree::PipelineTree;
pub use terminal::{PipelineViewState, TuiSession};
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
            SummaryStat {
                label: "errors".into(),
                value: counts.errors.to_string(),
                color: Some(Color::Red),
            },
            SummaryStat {
                label: "warnings".into(),
                value: counts.warnings.to_string(),
                color: Some(Color::Yellow),
            },
            SummaryStat {
                label: "notes".into(),
                value: counts.notes.to_string(),
                color: Some(Color::Blue),
            },
        ],
        slices,
    }
}
