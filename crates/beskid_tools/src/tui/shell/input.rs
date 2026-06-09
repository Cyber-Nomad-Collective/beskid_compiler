//! Keyboard and mouse routing for the unified shell.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Position;

use crate::pipeline::tui::log_input::{scroll_from_mouse, scroll_log, LogScrollEvent};
use crate::pipeline::tui::widgets::tree_click_at;
use crate::pipeline::tui::log_tabs::LogTab;
use crate::tui::input::{InputAction, InputEvent, InputResult};

use super::focus::{FocusTarget, OverlayKind, PaneFocus};
use super::state::{NavTarget, OverlayPanelFocus, ShellState};

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

pub fn handle_tests_overlay_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    if let InputEvent::Key(key) = event {
        if key.kind == KeyEventKind::Press
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
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
                    let height = state
                        .layout_rects
                        .tests_overlay
                        .map(|rect| rect.height.saturating_sub(2))
                        .unwrap_or(8);
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
                return if exit == Some(NavTarget::Exit) {
                    InputResult::Advance
                } else {
                    InputResult::Handled
                };
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
                let height = state
                    .layout_rects
                    .summary_overlay
                    .map(|rect| rect.height.saturating_sub(10))
                    .unwrap_or(8);
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

fn handle_key(state: &mut ShellState, key: KeyEvent) -> InputAction {
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
        _ => InputAction::None,
    }
}

fn handle_mouse(state: &mut ShellState, mouse: MouseEvent) -> InputAction {
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

fn switch_focus(state: &mut ShellState, target: FocusTarget) -> InputAction {
    state.focus = target;
    if let FocusTarget::Base(pane) = target {
        state.pane_focus = pane;
    }
    InputAction::Redraw
}

fn header_tab_at(state: &ShellState, position: Position) -> Option<FocusTarget> {
    let header = state.layout_rects.header;
    if position.y != header.y {
        return None;
    }
    let inner_x = position.x.saturating_sub(header.x + 1);
    let slot = inner_x.saturating_mul(3) / header.width.max(1);
    match slot {
        0 => Some(FocusTarget::Base(PaneFocus::Stage)),
        1 if state.tests_loaded => {
            Some(FocusTarget::Overlay(OverlayKind::Tests))
        }
        2 if state.summary_ready => {
            Some(FocusTarget::Overlay(OverlayKind::Summary))
        }
        _ => None,
    }
}

fn log_tab_at(state: &ShellState, position: Position) -> Option<LogTab> {
    let log = state.layout_rects.log;
    if position.y != log.y || !log.contains(position) {
        return None;
    }
    let inner_x = position.x.saturating_sub(log.x + 1);
    if inner_x < log.width / 2 {
        Some(LogTab::Build)
    } else {
        Some(LogTab::Semantic)
    }
}

fn pane_at(state: &ShellState, position: Position) -> Option<PaneFocus> {
    if state.layout_rects.log.contains(position) {
        return Some(PaneFocus::Log);
    }
    if state.layout_rects.detail.contains(position) {
        return Some(PaneFocus::Detail);
    }
    if state.layout_rects.stage.contains(position) {
        return Some(PaneFocus::Stage);
    }
    None
}

fn overlay_at(state: &ShellState, position: Position) -> Option<OverlayKind> {
    if state.overlay_visible(OverlayKind::Tests)
        && state
            .layout_rects
            .tests_overlay
            .is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Tests);
    }
    if state.overlay_visible(OverlayKind::Summary)
        && state
            .layout_rects
            .summary_overlay
            .is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Summary);
    }
    if state.overlay_visible(OverlayKind::Pckg)
        && state
            .layout_rects
            .pckg_overlay
            .is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Pckg);
    }
    if state.overlay_visible(OverlayKind::Templates)
        && state
            .layout_rects
            .templates_overlay
            .is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Templates);
    }
    None
}

fn scroll_active_log(state: &mut ShellState, event: LogScrollEvent) {
    let tab = state.log_tab;
    scroll_log(state.log_states.state_mut(tab), event);
}

fn route_vertical(state: &mut ShellState, up: bool) {
    match state.pane_focus {
        PaneFocus::Log => scroll_active_log(
            state,
            if up {
                LogScrollEvent::LineUp
            } else {
                LogScrollEvent::LineDown
            },
        ),
        PaneFocus::Detail => route_tree_key(state, up),
        _ => {}
    }
}

fn route_horizontal(state: &mut ShellState, left: bool) {
    if state.pane_focus == PaneFocus::Log {
        state.log_tab = if left {
            LogTab::Semantic
        } else {
            LogTab::Build
        };
        return;
    }
    if state.pane_focus == PaneFocus::Detail {
        route_tree_horizontal(state, !left);
    }
}

fn route_tree_key(state: &mut ShellState, down: bool) {
    let code = if down { KeyCode::Down } else { KeyCode::Up };
    let key = KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    state
        .tree_navigator
        .handle_key_event(key, &state.tree_nodes, &mut state.tree_state);
}

fn route_tree_horizontal(state: &mut ShellState, expand: bool) {
    let code = if expand { KeyCode::Right } else { KeyCode::Left };
    let key = KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    state
        .tree_navigator
        .handle_key_event(key, &state.tree_nodes, &mut state.tree_state);
}

fn scroll_tree(state: &mut ShellState, up: bool, amount: usize) {
    if up {
        state.tree_state.offset = state.tree_state.offset.saturating_sub(amount);
    } else {
        state.tree_state.offset = state.tree_state.offset.saturating_add(amount);
    }
}

fn step_list_selection(state: &mut ShellState, delta: i32) {
    if state.test_rows.is_empty() {
        return;
    }
    let current = state
        .test_list_state
        .selected()
        .unwrap_or(0)
        .min(state.test_rows.len().saturating_sub(1));
    let next = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        (current + delta as usize).min(state.test_rows.len() - 1)
    };
    state.test_list_state.select(Some(next));
    state.test_list_user_selected = true;
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
                let height = state
                    .layout_rects
                    .pckg_overlay
                    .map(|rect| rect.height.saturating_sub(8))
                    .unwrap_or(8);
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

fn step_pckg_selection(state: &mut ShellState, delta: i32) {
    if state.pckg.packages.is_empty() {
        return;
    }
    let current = state
        .pckg
        .list_state
        .selected()
        .unwrap_or(0)
        .min(state.pckg.packages.len().saturating_sub(1));
    let next = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        (current + delta as usize).min(state.pckg.packages.len() - 1)
    };
    state.pckg.list_state.select(Some(next));
    if let Some(id) = state.pckg.selected_package_id().map(str::to_owned) {
        state.pckg.pending_detail_fetch = Some(id);
    }
}

fn step_template_selection(state: &mut ShellState, delta: i32) {
    let row_count = state.templates.active_rows();
    if row_count == 0 {
        return;
    }
    let current = state
        .templates
        .list_state
        .selected()
        .unwrap_or(0)
        .min(row_count.saturating_sub(1));
    let next = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        (current + delta as usize).min(row_count - 1)
    };
    state.templates.list_state.select(Some(next));
    state.sync_template_detail_viewer();
}

fn queue_template_install(state: &mut ShellState) {
    let package_id = match state.templates.tab {
        crate::tui::shell::pane_state::TemplateListTab::Registry => {
            state.templates.selected_package_id()
        }
        crate::tui::shell::pane_state::TemplateListTab::Installed => state
            .templates
            .selected_package_id()
            .or_else(|| {
                state
                    .templates
                    .selected_short_name()
                    .map(crate::tui::panes::template_ops::resolve_package_id)
            }),
    };
    if let Some(package_id) = package_id {
        state.templates.pending_install = Some(package_id);
        state.templates.installing = true;
    }
}

fn list_row_index(area: ratatui::layout::Rect, position: Position, row_count: usize) -> Option<usize> {
    if row_count == 0 || !area.contains(position) {
        return None;
    }
    let inner_y = position.y.saturating_sub(area.y + 1);
    let index = inner_y as usize;
    if index < row_count {
        Some(index)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventState};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
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
        let mut state = ShellState {
            pane_focus: PaneFocus::Log,
            ..Default::default()
        };
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
        assert_eq!(
            pane_at(&state, Position::new(5, 5)),
            Some(PaneFocus::Stage)
        );
        assert_eq!(
            pane_at(&state, Position::new(50, 5)),
            Some(PaneFocus::Detail)
        );
        assert_eq!(pane_at(&state, Position::new(5, 14)), Some(PaneFocus::Log));
    }
}
