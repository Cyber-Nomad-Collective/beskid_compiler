//! Code outline TOC hover handling.

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::widgets::{
    code_widget::{foundation::CodeEvent, state::CodeState},
    document_viewer::extensions::outline_entry_at_position,
};

/// Updates outline hover state from a mouse move.
pub(crate) fn handle_outline_hover(
    event: MouseEvent,
    area: Rect,
    state: &mut CodeState,
) -> CodeEvent {
    let previous_hovered = state.outline_hovered;
    let previous_entry = state.outline_hovered_entry;
    let compact_entry = outline_entry_at_position(
        event.column,
        event.row,
        area,
        state.outline.len(),
        state.outline_hovered,
    );
    let expanded_entry =
        outline_entry_at_position(event.column, event.row, area, state.outline.len(), true);
    state.outline_hovered = compact_entry.is_some() || expanded_entry.is_some();
    state.outline_hovered_entry = expanded_entry;
    if previous_hovered != state.outline_hovered || previous_entry != state.outline_hovered_entry {
        CodeEvent::OutlineHoverChanged
    } else {
        CodeEvent::None
    }
}
