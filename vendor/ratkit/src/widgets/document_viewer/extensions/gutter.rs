//! Line-number gutter rendering for document viewers.

use ratatui::{
    buffer::Buffer,
    style::{Color, Style},
};

const GUTTER_BG: Color = Color::Rgb(10, 14, 20);
const LINE_NUMBER_FG: Color = Color::Rgb(70, 80, 100);
const SEPARATOR_FG: Color = Color::Rgb(45, 52, 70);

use crate::widgets::document_viewer::state::{DisplaySettings, ScrollState};

/// Calculates the line-number gutter width for a document.
pub fn gutter_width(line_count: usize, display: &DisplaySettings) -> usize {
    if !display.show_line_numbers {
        return 0;
    }
    line_count.max(1).to_string().len() + 5
}

/// Renders one line-number gutter cell.
pub fn render_gutter(
    x: u16,
    y: u16,
    width: usize,
    line_index: usize,
    scroll: &ScrollState,
    display: &DisplaySettings,
    buf: &mut Buffer,
) {
    if width == 0 {
        return;
    }
    let current_line = scroll.current_line_index();
    let number = if display.relative_line_numbers && line_index != current_line {
        line_index.abs_diff(current_line).to_string()
    } else {
        (line_index + 1).to_string()
    };
    let number_width = width.saturating_sub(3);
    let number_text = format!("{number:>number_width$} ");
    let number_style = Style::default().fg(LINE_NUMBER_FG).bg(GUTTER_BG);
    let separator_style = Style::default().fg(SEPARATOR_FG).bg(GUTTER_BG);
    buf.set_stringn(x, y, number_text, number_width + 1, number_style);
    buf.set_stringn(x + number_width as u16 + 1, y, "│ ", 2, separator_style);
}
