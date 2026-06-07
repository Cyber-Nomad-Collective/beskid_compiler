//! Shared terminal UI helpers (Ratatui session, timers, diagnostic summaries).

mod app;
mod diagnostics;
mod hyperlink;
mod logger_panel;
mod pipeline_tree;
mod terminal;
mod test_report;
mod test_table;
mod timer;
mod tree;

pub use diagnostics::{SeverityCounts, count_severities, format_severity_summary};
pub use hyperlink::{FileLineLink, hyperlinks_enabled};
pub use pipeline_tree::PipelineTree;
pub use terminal::{PipelineViewState, TuiSession};
pub use test_report::TestReportSummary;
pub use test_table::{TestRow, TestRowState, TestRunUi};
pub use timer::format_duration;
pub use tree::{format_phase_end, format_phase_start, format_work_unit};
