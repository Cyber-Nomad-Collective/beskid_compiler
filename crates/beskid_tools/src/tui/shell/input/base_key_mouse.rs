use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use crate::pipeline::tui::log_input::{LogScrollEvent, scroll_from_mouse};
use crate::pipeline::tui::log_tabs::LogTab;
use crate::pipeline::tui::widgets::tree_click_at;
use crate::tui::input::{InputAction, InputEvent, InputResult};

use super::super::focus::{FocusTarget, OverlayKind, PaneFocus};
use super::super::state::ShellState;
use super::focus_hit_test::{header_tab_at, log_tab_at, overlay_at, pane_at, switch_focus};
use super::navigation_scroll::{route_horizontal, route_vertical, scroll_active_log, scroll_tree};

pub fn handle_input_event(state: &mut ShellState, event: &InputEvent) -> InputAction {
    match event {
        InputEvent::Key(key) => handle_key(state, *key),
        InputEvent::Mouse(mouse) => handle_mouse(state, *mouse),
    }
}
pub fn handle_base_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    match handle_input_event(state, event) {
        InputAction::Quit => InputResult::Quit,
        InputAction::Advance => InputResult::Advance,
        InputAction::SkipNav => InputResult::SkipNav,
        InputAction::Redraw | InputAction::None => InputResult::Handled,
    }
}

pub(super) fn handle_key(state: &mut ShellState, key: KeyEvent) -> InputAction {
    if key.kind != KeyEventKind::Press {
        return InputAction::None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return InputAction::Quit;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            if state.awaiting_nav.is_some() && state.focus.is_base() {
                InputAction::SkipNav
            } else if state.focus == FocusTarget::Overlay(OverlayKind::Summary) {
                InputAction::Advance
            } else {
                InputAction::Quit
            }
        }
        KeyCode::Char(' ') | KeyCode::Enter if state.navigation_hint().is_some() => {
            let _ = state.advance_once();
            InputAction::Advance
        }
        KeyCode::Char('1') => switch_focus(state, FocusTarget::Base(PaneFocus::Stage)),
        KeyCode::Char('2') if state.tests_loaded => {
            state.set_overlay_visible(OverlayKind::Tests, true);
            switch_focus(state, FocusTarget::Overlay(OverlayKind::Tests))
        }
        KeyCode::Char('3') if state.summary_ready => {
            state.set_overlay_visible(OverlayKind::Summary, true);
            switch_focus(state, FocusTarget::Overlay(OverlayKind::Summary))
        }
        KeyCode::Char('4') => {
            state.set_overlay_visible(OverlayKind::Pckg, true);
            if !state.pckg.catalog_loaded {
                state.pckg.pending_catalog_refresh = true;
            }
            switch_focus(state, FocusTarget::Overlay(OverlayKind::Pckg))
        }
        KeyCode::Char('5') | KeyCode::Char('n')
            if state.shell_mode == crate::tui::shell::pane_state::ShellMode::ProjectWizard =>
        {
            state.set_overlay_visible(OverlayKind::Templates, true);
            switch_focus(state, FocusTarget::Overlay(OverlayKind::Templates))
        }
        KeyCode::Char('5') => {
            state.set_overlay_visible(OverlayKind::Templates, true);
            if !state.templates.catalog_loaded {
                state.templates.pending_catalog_refresh = true;
            }
            switch_focus(state, FocusTarget::Overlay(OverlayKind::Templates))
        }
        KeyCode::Tab => {
            if state.focus.is_base() {
                state.pane_focus = state.pane_focus.next();
                state.focus = FocusTarget::Base(state.pane_focus);
            }
            InputAction::Redraw
        }
        KeyCode::BackTab => {
            if state.focus.is_base() {
                state.pane_focus = state.pane_focus.next().next().next();
                state.focus = FocusTarget::Base(state.pane_focus);
            }
            InputAction::Redraw
        }
        KeyCode::Up => {
            route_vertical(state, true);
            InputAction::Redraw
        }
        KeyCode::Down => {
            route_vertical(state, false);
            InputAction::Redraw
        }
        KeyCode::Left => {
            route_horizontal(state, true);
            InputAction::Redraw
        }
        KeyCode::Right => {
            route_horizontal(state, false);
            InputAction::Redraw
        }
        KeyCode::PageUp => {
            if state.pane_focus == PaneFocus::Log {
                scroll_active_log(state, LogScrollEvent::PageUp);
            } else if state.pane_focus == PaneFocus::Detail {
                scroll_tree(state, true, 3);
            }
            InputAction::Redraw
        }
        KeyCode::PageDown => {
            if state.pane_focus == PaneFocus::Log {
                scroll_active_log(state, LogScrollEvent::PageDown);
            } else if state.pane_focus == PaneFocus::Detail {
                scroll_tree(state, false, 3);
            }
            InputAction::Redraw
        }
        KeyCode::End | KeyCode::Char('g') if state.pane_focus == PaneFocus::Log => {
            scroll_active_log(state, LogScrollEvent::FollowTail);
            InputAction::Redraw
        }
        KeyCode::Char('b') if state.pane_focus == PaneFocus::Log => {
            state.log_tab = LogTab::Build;
            InputAction::Redraw
        }
        KeyCode::Char('s') if state.pane_focus == PaneFocus::Log => {
            state.log_tab = LogTab::Semantic;
            InputAction::Redraw
        }
        KeyCode::Char('i') if state.pane_focus == PaneFocus::Log => {
            state.log_tab = LogTab::Incremental;
            InputAction::Redraw
        }
        KeyCode::Char('t') if state.pane_focus == PaneFocus::Log => {
            state.log_tab = LogTab::Traces;
            InputAction::Redraw
        }
        _ => InputAction::None,
    }
}

pub(super) fn handle_mouse(state: &mut ShellState, mouse: MouseEvent) -> InputAction {
    if let Some(scroll) = scroll_from_mouse(state.layout_rects.log, mouse) {
        state.pane_focus = PaneFocus::Log;
        state.focus = FocusTarget::Base(PaneFocus::Log);
        scroll_active_log(state, scroll);
        return InputAction::Redraw;
    }
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return InputAction::None;
    }
    let position = Position::new(mouse.column, mouse.row);
    if let Some(target) = header_tab_at(state, position) {
        return switch_focus(state, target);
    }
    if let Some(tab) = log_tab_at(state, position) {
        state.pane_focus = PaneFocus::Log;
        state.focus = FocusTarget::Base(PaneFocus::Log);
        state.log_tab = tab;
        return InputAction::Redraw;
    }
    if let Some(pane) = pane_at(state, position) {
        state.pane_focus = pane;
        state.focus = FocusTarget::Base(pane);
    }
    if state.pane_focus == PaneFocus::Detail {
        tree_click_at(state.layout_rects.detail, mouse, &state.tree_nodes, &mut state.tree_state);
        return InputAction::Redraw;
    }
    if let Some(kind) = overlay_at(state, position) {
        state.set_overlay_visible(kind, true);
        return switch_focus(state, FocusTarget::Overlay(kind));
    }
    InputAction::Redraw
}
