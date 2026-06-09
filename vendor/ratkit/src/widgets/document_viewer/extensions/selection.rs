//! Selection overlay helpers for document viewers.

use ratatui::style::{Color, Style};

use crate::widgets::document_viewer::state::SelectionState;

/// Returns true when a zero-based line is selected.
pub fn is_line_selected(selection: Option<&SelectionState>, line_index: usize) -> bool {
    selection
        .and_then(SelectionState::selected_range)
        .is_some_and(|range| range.contains(&line_index))
}

/// Returns the style used for selected lines.
pub fn selected_line_style(is_selected: bool) -> Style {
    if is_selected {
        Style::default().bg(Color::Rgb(54, 64, 74))
    } else {
        Style::default()
    }
}
