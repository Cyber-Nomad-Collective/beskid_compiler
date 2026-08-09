use crossterm::event::{KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::Position;

use crate::tui::input::{InputEvent, InputResult};

use super::super::state::{NavTarget, OverlayPanelFocus, ShellState};
use super::selection_templates::{
    list_row_index, queue_template_install, step_list_selection, step_pckg_selection, step_template_selection,
};

pub fn handle_tests_overlay_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    if let InputEvent::Key(key) = event {
        if key.kind == KeyEventKind::Press && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            return InputResult::CloseOverlay;
        }
        if key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char(' ') | KeyCode::Enter)
            && state.navigation_hint().is_some()
        {
            let _ = state.advance_once();
            return InputResult::Advance;
        }
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => {
                    state.overlay_panel_focus = match state.overlay_panel_focus {
                        OverlayPanelFocus::List => OverlayPanelFocus::Code,
                        OverlayPanelFocus::Code => OverlayPanelFocus::List,
                    };
                    return InputResult::Handled;
                }
                KeyCode::Up if state.overlay_panel_focus == OverlayPanelFocus::Code => {
                    state.code_viewer.scroll_up();
                    return InputResult::Handled;
                }
                KeyCode::Down if state.overlay_panel_focus == OverlayPanelFocus::Code => {
                    let height =
                        state.layout_rects.tests_overlay.map(|rect| rect.height.saturating_sub(2)).unwrap_or(8);
                    state.code_viewer.scroll_down(height);
                    return InputResult::Handled;
                }
                KeyCode::Up => {
                    step_list_selection(state, -1);
                    state.sync_code_viewer_for_selection();
                    return InputResult::Handled;
                }
                KeyCode::Down => {
                    step_list_selection(state, 1);
                    state.sync_code_viewer_for_selection();
                    return InputResult::Handled;
                }
                KeyCode::PageUp => {
                    step_list_selection(state, -5);
                    state.sync_code_viewer_for_selection();
                    return InputResult::Handled;
                }
                KeyCode::PageDown => {
                    step_list_selection(state, 5);
                    state.sync_code_viewer_for_selection();
                    return InputResult::Handled;
                }
                _ => {}
            }
        }
    }
    if let InputEvent::Mouse(mouse) = event
        && mouse.kind == MouseEventKind::Down(MouseButton::Left)
        && let Some(rect) = state.layout_rects.tests_overlay
    {
        let position = Position::new(mouse.column, mouse.row);
        if let Some(index) = list_row_index(rect, position, state.test_rows.len()) {
            state.test_list_state.select(Some(index));
            state.test_list_user_selected = true;
            state.overlay_panel_focus = OverlayPanelFocus::List;
            state.sync_code_viewer_for_test(index);
            return InputResult::Handled;
        }
    }
    InputResult::Bubble
}
pub fn handle_summary_overlay_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    if let InputEvent::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char(' ') | KeyCode::Enter => {
                let exit = state.advance_once();
                return if exit == Some(NavTarget::Exit) { InputResult::Advance } else { InputResult::Handled };
            }
            KeyCode::Tab | KeyCode::BackTab => {
                state.overlay_panel_focus = match state.overlay_panel_focus {
                    OverlayPanelFocus::List => OverlayPanelFocus::Code,
                    OverlayPanelFocus::Code => OverlayPanelFocus::List,
                };
                return InputResult::Handled;
            }
            KeyCode::Up if state.overlay_panel_focus == OverlayPanelFocus::Code => {
                state.code_viewer.scroll_up();
                return InputResult::Handled;
            }
            KeyCode::Down if state.overlay_panel_focus == OverlayPanelFocus::Code => {
                let height = state.layout_rects.summary_overlay.map(|rect| rect.height.saturating_sub(10)).unwrap_or(8);
                state.code_viewer.scroll_down(height);
                return InputResult::Handled;
            }
            KeyCode::Up => {
                step_summary_explorer(state, -1);
                return InputResult::Handled;
            }
            KeyCode::Down => {
                step_summary_explorer(state, 1);
                return InputResult::Handled;
            }
            _ => {}
        }
    }
    InputResult::Bubble
}

fn step_summary_explorer(state: &mut ShellState, delta: i32) {
    let failed_len = state.failed_test_indices().len();
    if failed_len == 0 {
        return;
    }
    let next = if delta < 0 {
        state.summary_explorer_index.saturating_sub(1)
    } else {
        (state.summary_explorer_index + 1).min(failed_len - 1)
    };
    state.summary_explorer_index = next;
    state.sync_summary_explorer();
}

pub fn handle_simple_overlay_input(event: &InputEvent, _state: &mut ShellState) -> InputResult {
    if let InputEvent::Key(key) = event
        && key.kind == KeyEventKind::Press
        && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
    {
        return InputResult::CloseOverlay;
    }
    InputResult::Bubble
}

pub fn handle_pckg_overlay_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    if let InputEvent::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return InputResult::CloseOverlay,
            KeyCode::Char('r') => {
                state.pckg.catalog_loaded = false;
                state.pckg.error = None;
                state.pckg.pending_catalog_refresh = true;
                return InputResult::Handled;
            }
            KeyCode::Up if state.overlay_panel_focus == OverlayPanelFocus::Code => {
                state.code_viewer.scroll_up();
                return InputResult::Handled;
            }
            KeyCode::Down if state.overlay_panel_focus == OverlayPanelFocus::Code => {
                let height = state.layout_rects.pckg_overlay.map(|rect| rect.height.saturating_sub(8)).unwrap_or(8);
                state.code_viewer.scroll_down(height);
                return InputResult::Handled;
            }
            KeyCode::Up => {
                step_pckg_selection(state, -1);
                return InputResult::Handled;
            }
            KeyCode::Down => {
                step_pckg_selection(state, 1);
                return InputResult::Handled;
            }
            KeyCode::PageUp => {
                step_pckg_selection(state, -8);
                return InputResult::Handled;
            }
            KeyCode::PageDown => {
                step_pckg_selection(state, 8);
                return InputResult::Handled;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                state.overlay_panel_focus = match state.overlay_panel_focus {
                    OverlayPanelFocus::List => OverlayPanelFocus::Code,
                    OverlayPanelFocus::Code => OverlayPanelFocus::List,
                };
                return InputResult::Handled;
            }
            _ => {}
        }
    }
    InputResult::Bubble
}

pub fn handle_templates_overlay_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    if let InputEvent::Key(key) = event
        && key.kind == KeyEventKind::Press
    {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return InputResult::CloseOverlay,
            KeyCode::Tab => {
                state.templates.tab = match state.templates.tab {
                    crate::tui::shell::pane_state::TemplateListTab::Installed => {
                        crate::tui::shell::pane_state::TemplateListTab::Registry
                    }
                    crate::tui::shell::pane_state::TemplateListTab::Registry => {
                        crate::tui::shell::pane_state::TemplateListTab::Installed
                    }
                };
                state.templates.list_state.select(None);
                state.sync_template_detail_viewer();
                return InputResult::Handled;
            }
            KeyCode::Char('r') => {
                state.templates.catalog_loaded = false;
                state.templates.error = None;
                state.templates.pending_catalog_refresh = true;
                return InputResult::Handled;
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                queue_template_install(state);
                return InputResult::Handled;
            }
            KeyCode::Up => {
                step_template_selection(state, -1);
                return InputResult::Handled;
            }
            KeyCode::Down => {
                step_template_selection(state, 1);
                return InputResult::Handled;
            }
            KeyCode::PageUp => {
                step_template_selection(state, -8);
                return InputResult::Handled;
            }
            KeyCode::PageDown => {
                step_template_selection(state, 8);
                return InputResult::Handled;
            }
            _ => {}
        }
    }
    InputResult::Bubble
}
