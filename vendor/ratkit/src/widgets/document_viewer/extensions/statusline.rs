//! Statusline rendering for document viewers.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::primitives::statusline::{StatusLineStacked, SLANT_BL_TR, SLANT_TL_BR};

/// Renders a one-line viewer statusline.
pub fn render_statusline(area: Rect, buf: &mut Buffer, text: &str) {
    if area.is_empty() {
        return;
    }
    let mode_bg = Color::Rgb(97, 175, 239);
    let mode_fg = Color::Black;
    let file_bg = Color::Rgb(58, 58, 58);
    let position_bg = Color::Rgb(171, 178, 191);
    let position_fg = Color::Black;
    let statusline = StatusLineStacked::new()
        .start(
            Span::from(" NORMAL ").style(
                Style::new()
                    .fg(mode_fg)
                    .bg(mode_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::from(SLANT_TL_BR).style(Style::new().fg(mode_bg).bg(file_bg)),
        )
        .end(
            Span::from(format!(" {text} ")).style(Style::new().fg(position_fg).bg(position_bg)),
            Span::from(SLANT_BL_TR).style(Style::new().fg(position_bg)),
        );
    ratatui::widgets::Widget::render(statusline, area, buf);
}
