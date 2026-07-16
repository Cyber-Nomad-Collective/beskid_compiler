//! Compile debugger overlay: phase timeline, incremental log, traces.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::shell::widgets::{CompileDebugTab, draw_compile_debug_panel};
use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::input;
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
    let effects = Vec::new();
    if let ShellMessage::SetOverlayVisible {
        kind: OverlayKind::CompileDebug,
        visible: true,
    } = msg
    {
        state.set_overlay_visible(OverlayKind::CompileDebug, true);
        state.focus_overlay(OverlayKind::CompileDebug);
    }
    effects
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    input::handle_simple_overlay_input(event, state)
}

pub fn render(area: Rect, frame: &mut Frame, state: &mut ShellState) {
    draw_compile_debug_panel(frame, area, None, state, CompileDebugTab::Timeline);
}
