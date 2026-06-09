//! Mouse navigation for shared document viewers.

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::widgets::document_viewer::state::{ScrollState, SelectionState};

/// Applies a mouse event to shared viewer scroll and selection state.
pub fn handle_viewer_mouse(
    event: MouseEvent,
    area: Rect,
    scroll: &mut ScrollState,
    selection: &mut SelectionState,
) -> bool {
    match event.kind {
        MouseEventKind::ScrollDown => {
            scroll.scroll_down(3);
            true
        }
        MouseEventKind::ScrollUp => {
            scroll.scroll_up(3);
            true
        }
        MouseEventKind::Down(_) if contains_point(area, event.column, event.row) => {
            let row = event.row.saturating_sub(area.y) as usize;
            let line = scroll.effective_offset().saturating_add(row);
            scroll.scroll_to(line);
            selection.select_line(scroll.current_line_index());
            true
        }
        MouseEventKind::Drag(_) if contains_point(area, event.column, event.row) => {
            let row = event.row.saturating_sub(area.y) as usize;
            let line = scroll.effective_offset().saturating_add(row);
            selection.extend_to(line.min(scroll.total_lines.saturating_sub(1)));
            true
        }
        _ => false,
    }
}

/// Returns true when a coordinate is inside an area.
fn contains_point(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && y >= area.y
        && x < area.x.saturating_add(area.width)
        && y < area.y.saturating_add(area.height)
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyModifiers, MouseButton};

    use super::*;

    #[test]
    fn click_selects_viewer_line() {
        let mut scroll = ScrollState::default();
        scroll.update_total_lines(20);
        scroll.viewport_height = 10;
        scroll.set_offset(5);
        let mut selection = SelectionState::default();
        let event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        assert!(handle_viewer_mouse(
            event,
            Rect::new(0, 0, 10, 10),
            &mut scroll,
            &mut selection
        ));
        assert_eq!(scroll.current_line, 9);
        assert!(selection.selected_range().unwrap().contains(&8));
    }

    #[test]
    fn scroll_wheel_moves_viewer() {
        let mut scroll = ScrollState::default();
        scroll.update_total_lines(20);
        scroll.viewport_height = 10;
        let mut selection = SelectionState::default();
        let event = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        };
        assert!(handle_viewer_mouse(
            event,
            Rect::new(0, 0, 10, 10),
            &mut scroll,
            &mut selection
        ));
        assert_eq!(scroll.effective_offset(), 3);
    }
}
