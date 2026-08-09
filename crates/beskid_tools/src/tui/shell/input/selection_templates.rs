use ratatui::layout::Position;

use super::super::state::ShellState;

pub(super) fn step_list_selection(state: &mut ShellState, delta: i32) {
    if state.test_rows.is_empty() {
        return;
    }
    let current = state.test_list_state.selected().unwrap_or(0).min(state.test_rows.len().saturating_sub(1));
    let next = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        (current + delta as usize).min(state.test_rows.len() - 1)
    };
    state.test_list_state.select(Some(next));
    state.test_list_user_selected = true;
}
pub(super) fn step_pckg_selection(state: &mut ShellState, delta: i32) {
    if state.pckg.packages.is_empty() {
        return;
    }
    let current = state.pckg.list_state.selected().unwrap_or(0).min(state.pckg.packages.len().saturating_sub(1));
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

pub(super) fn step_template_selection(state: &mut ShellState, delta: i32) {
    let row_count = state.templates.active_rows();
    if row_count == 0 {
        return;
    }
    let current = state.templates.list_state.selected().unwrap_or(0).min(row_count.saturating_sub(1));
    let next = if delta < 0 {
        current.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        (current + delta as usize).min(row_count - 1)
    };
    state.templates.list_state.select(Some(next));
    state.sync_template_detail_viewer();
}

pub(super) fn queue_template_install(state: &mut ShellState) {
    let package_id = match state.templates.tab {
        crate::tui::shell::pane_state::TemplateListTab::Registry => state.templates.selected_package_id(),
        crate::tui::shell::pane_state::TemplateListTab::Installed => state
            .templates
            .selected_package_id()
            .or_else(|| state.templates.selected_short_name().map(crate::tui::panes::template_ops::resolve_package_id)),
    };
    if let Some(package_id) = package_id {
        state.templates.pending_install = Some(package_id);
        state.templates.installing = true;
    }
}

pub(super) fn list_row_index(area: ratatui::layout::Rect, position: Position, row_count: usize) -> Option<usize> {
    if row_count == 0 || !area.contains(position) {
        return None;
    }
    let inner_y = position.y.saturating_sub(area.y + 1);
    let index = inner_y as usize;
    if index < row_count { Some(index) } else { None }
}
