//! Scrollbar rendering for document viewers.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

use crate::widgets::document_viewer::state::ScrollState;

/// Renders a simple vertical scrollbar for a viewport.
pub fn render_scrollbar(area: Rect, buf: &mut Buffer, scroll: &ScrollState) {
    if area.is_empty() || scroll.total_lines <= area.height as usize {
        return;
    }
    let thumb_height = thumb_height(area.height, scroll.total_lines);
    let thumb_top = thumb_top(area.height, thumb_height, scroll);
    for row in 0..area.height {
        let is_thumb = row >= thumb_top && row < thumb_top.saturating_add(thumb_height);
        let symbol = if is_thumb { "█" } else { "│" };
        let color = if is_thumb {
            Color::Gray
        } else {
            Color::DarkGray
        };
        buf.set_string(area.x, area.y + row, symbol, Style::default().fg(color));
    }
}

/// Calculates scrollbar thumb height.
fn thumb_height(view_height: u16, total_lines: usize) -> u16 {
    let ratio = view_height as f64 / total_lines.max(1) as f64;
    ((view_height as f64 * ratio).ceil() as u16).clamp(1, view_height)
}

/// Calculates scrollbar thumb top row.
fn thumb_top(view_height: u16, thumb_height: u16, scroll: &ScrollState) -> u16 {
    let max_top = view_height.saturating_sub(thumb_height);
    let max_offset = scroll
        .total_lines
        .saturating_sub(view_height as usize)
        .max(1);
    let ratio = scroll.offset as f64 / max_offset as f64;
    (ratio * max_top as f64).round() as u16
}
