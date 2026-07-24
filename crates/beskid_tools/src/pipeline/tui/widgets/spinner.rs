//! Animated spinners via [`tui-spinner`] (replaces hand-rolled braille frames).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tui_spinner::{BarMotion, BarSpinner, FluxSpinner};

/// Compact 1×1 header glyph (status bar).
pub fn draw_status_spinner(frame: &mut Frame, area: Rect, tick: u64) {
    frame.render_widget(FluxSpinner::new(tick).color(Color::Cyan), area);
}

/// Indeterminate stage bar while pipeline work is in flight.
pub fn draw_stage_bar_spinner(frame: &mut Frame, area: Rect, tick: u64) {
    frame.render_widget(
        BarSpinner::new(tick).motion(BarMotion::Loop).arc_color(Color::Cyan).dim_color(Color::DarkGray),
        area,
    );
}
