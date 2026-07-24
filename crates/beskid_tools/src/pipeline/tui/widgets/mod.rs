//! Thin wrappers around Ratatui built-ins and ecosystem widgets.
//!
//! | Pane / control | Crate |
//! |---|---|
//! | Pipeline tree | shell `TreeView` (ratkit widget) |
//! | Build log | [`tui-logger`] |
//! | Summary chart | [`tui-piechart`] |
//! | In-flight spinners | [`tui-spinner`] |
//! | Progress gauges | Ratatui [`Gauge`] |
//! | Test list | Ratatui [`List`] |
//! | Stats table | Ratatui [`Table`] |

pub mod context_bar;
pub mod log_panel;
pub mod pipeline_tree_view;
pub mod progress_footer;
pub mod spinner;
pub mod stage_panel;
pub mod summary_panel;

pub use context_bar::draw_context_bar;
pub use log_panel::{draw_log_panel, draw_tabbed_log_panel, init_session_logger, shutdown_session_logger};
pub use pipeline_tree_view::{draw_pipeline_tree, tree_click_at};
pub use progress_footer::draw_progress_footer;
pub use stage_panel::draw_stage_panel;
pub use summary_panel::{draw_summary_chart_panel, draw_summary_headline_footer};
