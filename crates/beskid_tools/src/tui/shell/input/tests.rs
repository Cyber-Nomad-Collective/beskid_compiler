use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::layout::Position;

use crate::pipeline::tui::log_tabs::LogTab;

use super::super::focus::PaneFocus;
use super::super::state::ShellState;
use super::base_key_mouse::handle_key;
use super::focus_hit_test::pane_at;

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
}
#[test]
fn tab_cycles_pane_focus() {
    let mut state = ShellState::default();
    assert_eq!(state.pane_focus, PaneFocus::Stage);
    handle_key(&mut state, press(KeyCode::Tab));
    assert_eq!(state.pane_focus, PaneFocus::Detail);
    handle_key(&mut state, press(KeyCode::Tab));
    assert_eq!(state.pane_focus, PaneFocus::Log);
}

#[test]
fn log_tab_keys_switch_stream() {
    let mut state = ShellState { pane_focus: PaneFocus::Log, ..Default::default() };
    handle_key(&mut state, press(KeyCode::Char('s')));
    assert_eq!(state.log_tab, LogTab::Semantic);
    handle_key(&mut state, press(KeyCode::Char('b')));
    assert_eq!(state.log_tab, LogTab::Build);
}

#[test]
fn pane_at_hit_tests_layout_rects() {
    let state = ShellState {
        layout_rects: crate::tui::shell::state::LayoutRects {
            header: ratatui::layout::Rect::new(0, 0, 80, 3),
            stage: ratatui::layout::Rect::new(0, 3, 40, 10),
            detail: ratatui::layout::Rect::new(40, 3, 40, 10),
            log: ratatui::layout::Rect::new(0, 13, 80, 6),
            footer: ratatui::layout::Rect::new(0, 19, 80, 4),
            chrome: ratatui::layout::Rect::new(0, 23, 80, 1),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(pane_at(&state, Position::new(5, 5)), Some(PaneFocus::Stage));
    assert_eq!(pane_at(&state, Position::new(50, 5)), Some(PaneFocus::Detail));
    assert_eq!(pane_at(&state, Position::new(5, 14)), Some(PaneFocus::Log));
}
