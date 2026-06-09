//! Optional rendering extensions for read-only document viewers.

pub mod gutter;
pub mod outline;
pub mod scrollbar;
pub mod selection;
pub mod statusline;

pub use gutter::{gutter_width, render_gutter};
pub use outline::{outline_entry_at_position, outline_overlay_area, render_outline};
pub use scrollbar::render_scrollbar;
pub use selection::{is_line_selected, selected_line_style};
pub use statusline::render_statusline;
