use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use crate::pipeline::tui::log_input::{LogScrollEvent, scroll_log};

use super::super::focus::PaneFocus;
use super::super::state::ShellState;

pub(super) fn scroll_active_log(state: &mut ShellState, event: LogScrollEvent) {
    let tab = state.log_tab;
    scroll_log(state.log_states.state_mut(tab), event);
}
pub(super) fn route_vertical(state: &mut ShellState, up: bool) {
    match state.pane_focus {
        PaneFocus::Log => scroll_active_log(state, if up { LogScrollEvent::LineUp } else { LogScrollEvent::LineDown }),
        PaneFocus::Detail => route_tree_key(state, up),
        _ => {}
    }
}

pub(super) fn route_horizontal(state: &mut ShellState, left: bool) {
    if state.pane_focus == PaneFocus::Log {
        state.log_tab = if left { state.log_tab.prev() } else { state.log_tab.next() };
        return;
    }
    if state.pane_focus == PaneFocus::Detail {
        route_tree_horizontal(state, !left);
    }
}

pub(super) fn route_tree_key(state: &mut ShellState, down: bool) {
    let code = if down { KeyCode::Down } else { KeyCode::Up };
    let key = KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE };
    state.tree_navigator.handle_key_event(key, &state.tree_nodes, &mut state.tree_state);
}

pub(super) fn route_tree_horizontal(state: &mut ShellState, expand: bool) {
    let code = if expand { KeyCode::Right } else { KeyCode::Left };
    let key = KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE };
    state.tree_navigator.handle_key_event(key, &state.tree_nodes, &mut state.tree_state);
}

pub(super) fn scroll_tree(state: &mut ShellState, up: bool, amount: usize) {
    if up {
        state.tree_state.offset = state.tree_state.offset.saturating_sub(amount);
    } else {
        state.tree_state.offset = state.tree_state.offset.saturating_add(amount);
    }
}
