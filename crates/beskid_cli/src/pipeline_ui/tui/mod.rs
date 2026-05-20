//! Shared terminal UI helpers (boxes, timers, diagnostic summaries).

mod box_draw;
mod diagnostics;
mod hyperlink;
mod test_table;
mod timer;

pub use box_draw::{write_box_bottom, write_box_line, write_box_top, BOX_INNER_WIDTH};
pub use diagnostics::{count_severities, format_severity_summary, SeverityCounts};
pub use hyperlink::{FileLineLink, hyperlinks_enabled};
pub use test_table::{TestRowState, TestRunUi};
pub use timer::format_duration;
