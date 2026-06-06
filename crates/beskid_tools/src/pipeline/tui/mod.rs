//! Shared terminal UI helpers (boxes, timers, diagnostic summaries).

mod box_draw;
mod diagnostics;
mod hyperlink;
mod test_table;
mod timer;

pub use box_draw::{BOX_INNER_WIDTH, write_box_bottom, write_box_line, write_box_top};
pub use diagnostics::{SeverityCounts, count_severities, format_severity_summary};
pub use hyperlink::{FileLineLink, hyperlinks_enabled};
pub use test_table::{TestRowState, TestRunUi};
pub use timer::format_duration;
