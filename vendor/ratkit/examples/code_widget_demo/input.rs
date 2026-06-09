//! Input handling for the CodeWidget demo.

use std::time::{Duration, Instant};

use crossterm::event::{
    KeyCode, KeyEvent as CrosstermKeyEvent, KeyEventState, MouseEvent as CrosstermMouseEvent,
};
use ratkit::{widgets::CodeEvent, CoordinatorAction, LayoutResult};

use crate::app::CodeWidgetDemo;

/// Handles keyboard events for the demo app.
pub fn handle_keyboard(
    app: &mut CodeWidgetDemo,
    key: ratkit::KeyboardEvent,
) -> LayoutResult<CoordinatorAction> {
    if !key.is_key_down() {
        return Ok(CoordinatorAction::Continue);
    }
    if should_quit(&key) {
        return Ok(CoordinatorAction::Quit);
    }
    if key.key_code == KeyCode::Char(']') {
        app.state.display.show_outline = !app.state.display.show_outline;
        return Ok(CoordinatorAction::Redraw);
    }
    let key_event = CrosstermKeyEvent {
        code: key.key_code,
        modifiers: key.modifiers,
        kind: key.kind,
        state: KeyEventState::NONE,
    };
    app.widget().handle_key(key_event, &mut app.state);
    Ok(CoordinatorAction::Redraw)
}

/// Returns true when a keyboard event should quit the demo.
fn should_quit(key: &ratkit::KeyboardEvent) -> bool {
    key.key_code == KeyCode::Char('q')
        || (key.key_code == KeyCode::Char('c')
            && key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL))
}

/// Handles mouse events for the demo app.
pub fn handle_mouse(
    app: &mut CodeWidgetDemo,
    mouse: ratkit::MouseEvent,
) -> LayoutResult<CoordinatorAction> {
    let is_moved = matches!(mouse.kind, crossterm::event::MouseEventKind::Moved);
    app.mouse_x = mouse.x();
    app.mouse_y = mouse.y();
    if is_moved && app.last_move_processed.elapsed() < Duration::from_millis(24) {
        return Ok(CoordinatorAction::Continue);
    }
    app.last_move_processed = Instant::now();
    let mouse_event = CrosstermMouseEvent {
        kind: mouse.kind,
        column: mouse.column,
        row: mouse.row,
        modifiers: mouse.modifiers,
    };
    let code_event = app
        .widget()
        .handle_mouse(mouse_event, app.code_area, &mut app.state);
    Ok(if is_moved && matches!(code_event, CodeEvent::None) {
        CoordinatorAction::Continue
    } else {
        CoordinatorAction::Redraw
    })
}
