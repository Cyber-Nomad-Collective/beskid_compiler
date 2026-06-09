//! Code outline TOC click handling.

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;

use crate::widgets::{
    code_widget::{foundation::CodeEvent, state::CodeState},
    document_viewer::extensions::outline_entry_at_position,
};

/// Jumps to an outline entry from a TOC click.
pub(crate) fn handle_outline_click(
    event: MouseEvent,
    area: Rect,
    state: &mut CodeState,
) -> Option<CodeEvent> {
    let entry = outline_entry_at_position(
        event.column,
        event.row,
        area,
        state.outline.len(),
        state.outline_hovered,
    )
    .or_else(|| {
        outline_entry_at_position(event.column, event.row, area, state.outline.len(), true)
    })?;
    let target_line = state.outline.get(entry)?.line.max(1);
    state.scroll.set_current_line(target_line);
    state.scroll.set_offset(target_line.saturating_sub(2));
    state.selection.clear();
    state.outline_hovered = true;
    state.outline_hovered_entry = Some(entry);
    Some(CodeEvent::Navigated { line: target_line })
}
