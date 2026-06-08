//! Thin wrappers around ratatui built-ins and ecosystem widgets.

pub mod context_bar;
pub mod log_panel;
pub mod pipeline_tree_view;
pub mod progress_footer;
pub mod stage_panel;
pub mod summary_panel;

pub use context_bar::draw_context_bar;
pub use log_panel::{draw_log_panel, init_session_logger, shutdown_session_logger};
pub use pipeline_tree_view::draw_pipeline_tree;
pub use progress_footer::draw_progress_footer;
pub use stage_panel::draw_stage_panel;
pub use summary_panel::{
    draw_summary_chart_panel, draw_summary_headline_footer, draw_summary_panel,
};
