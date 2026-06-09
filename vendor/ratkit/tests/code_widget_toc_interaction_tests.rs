#![cfg(feature = "code-widget")]

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratkit::widgets::{document_viewer::DocumentOutlineItem, CodeEvent, CodeState, CodeWidget};

/// Builds code state with a navigable outline.
fn code_state_with_outline() -> CodeState {
    let mut state = CodeState::default();
    state.display.show_outline = true;
    state.scroll.update_total_lines(20);
    state.scroll.viewport_height = 8;
    state.outline = vec![
        DocumentOutlineItem::new("Counter", 1, 0, "struct"),
        DocumentOutlineItem::new("increment", 10, 1, "function"),
    ];
    state
}

/// Creates a mouse event at the requested point.
fn mouse_event(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Verifies moving over the compact TOC expands and highlights an entry.
#[test]
fn hovering_code_toc_updates_outline_hover() {
    let widget = CodeWidget::default().show_outline(true);
    let mut state = code_state_with_outline();
    let event = mouse_event(MouseEventKind::Moved, 67, 2);
    let result = widget.handle_mouse(event, Rect::new(0, 0, 80, 10), &mut state);
    assert_eq!(result, CodeEvent::OutlineHoverChanged);
    assert!(state.outline_hovered);
    assert_eq!(state.outline_hovered_entry, Some(0));
}

/// Verifies clicking a TOC entry jumps to its source line.
#[test]
fn clicking_code_toc_jumps_to_outline_line() {
    let widget = CodeWidget::default().show_outline(true);
    let mut state = code_state_with_outline();
    state.outline_hovered = true;
    let event = mouse_event(MouseEventKind::Down(MouseButton::Left), 67, 3);
    let result = widget.handle_mouse(event, Rect::new(0, 0, 80, 10), &mut state);
    assert_eq!(result, CodeEvent::Navigated { line: 10 });
    assert_eq!(state.scroll.current_line, 10);
}
