//! Log panel scroll routing for [`tui_logger::TuiWidgetState`].

use crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use tui_logger::{TuiWidgetEvent, TuiWidgetState};

/// Scroll the build log (arrows, page keys, wheel). Returns true when follow-tail mode resumes.
pub fn scroll_log(state: &TuiWidgetState, event: LogScrollEvent) -> bool {
    match event {
        LogScrollEvent::FollowTail => {
            state.transition(TuiWidgetEvent::EscapeKey);
            true
        }
        LogScrollEvent::LineUp | LogScrollEvent::PageUp => {
            state.transition(TuiWidgetEvent::PrevPageKey);
            false
        }
        LogScrollEvent::LineDown | LogScrollEvent::PageDown => {
            state.transition(TuiWidgetEvent::NextPageKey);
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogScrollEvent {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    FollowTail,
}

pub fn scroll_from_mouse(area: Rect, mouse: MouseEvent) -> Option<LogScrollEvent> {
    let position = Position::new(mouse.column, mouse.row);
    if !area.contains(position) {
        return None;
    }
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(LogScrollEvent::LineUp),
        MouseEventKind::ScrollDown => Some(LogScrollEvent::LineDown),
        _ => None,
    }
}
