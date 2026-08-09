use ratatui::layout::Position;

use crate::pipeline::tui::log_tabs::LogTab;
use crate::tui::input::InputAction;

use super::super::focus::{FocusTarget, OverlayKind, PaneFocus};
use super::super::state::ShellState;

pub(super) fn switch_focus(state: &mut ShellState, target: FocusTarget) -> InputAction {
    state.focus = target;
    if let FocusTarget::Base(pane) = target {
        state.pane_focus = pane;
    }
    InputAction::Redraw
}
pub(super) fn header_tab_at(state: &ShellState, position: Position) -> Option<FocusTarget> {
    let header = state.layout_rects.header;
    if position.y != header.y {
        return None;
    }
    let inner_x = position.x.saturating_sub(header.x + 1);
    let slot = inner_x.saturating_mul(3) / header.width.max(1);
    match slot {
        0 => Some(FocusTarget::Base(PaneFocus::Stage)),
        1 if state.tests_loaded => Some(FocusTarget::Overlay(OverlayKind::Tests)),
        2 if state.summary_ready => Some(FocusTarget::Overlay(OverlayKind::Summary)),
        _ => None,
    }
}

pub(super) fn log_tab_at(state: &ShellState, position: Position) -> Option<LogTab> {
    let log = state.layout_rects.log;
    if position.y != log.y || !log.contains(position) {
        return None;
    }
    let inner_x = position.x.saturating_sub(log.x + 1);
    let slot = inner_x.saturating_mul(LogTab::ALL.len() as u16) / log.width.max(1);
    LogTab::ALL.get(slot as usize).copied()
}

pub(super) fn pane_at(state: &ShellState, position: Position) -> Option<PaneFocus> {
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

pub(super) fn overlay_at(state: &ShellState, position: Position) -> Option<OverlayKind> {
    if state.overlay_visible(OverlayKind::Tests)
        && state.layout_rects.tests_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Tests);
    }
    if state.overlay_visible(OverlayKind::Summary)
        && state.layout_rects.summary_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Summary);
    }
    if state.overlay_visible(OverlayKind::Pckg) && state.layout_rects.pckg_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Pckg);
    }
    if state.overlay_visible(OverlayKind::Templates)
        && state.layout_rects.templates_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Templates);
    }
    if state.overlay_visible(OverlayKind::CompileDebug)
        && state.layout_rects.compile_debug_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::CompileDebug);
    }
    if state.overlay_visible(OverlayKind::Graph)
        && state.layout_rects.graph_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Graph);
    }
    if state.overlay_visible(OverlayKind::Settings)
        && state.layout_rects.settings_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Settings);
    }
    if state.overlay_visible(OverlayKind::Analysis)
        && state.layout_rects.analysis_overlay.is_some_and(|r| r.contains(position))
    {
        return Some(OverlayKind::Analysis);
    }
    None
}
