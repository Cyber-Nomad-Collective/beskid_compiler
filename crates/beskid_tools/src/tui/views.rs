//! View update, input, and render dispatch (replaces the custom Screen registry).

use ratatui::Frame;

use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::screens::{
    pckg_overlay, pipeline_compile, summary_overlay, templates_overlay, tests_overlay,
};
use crate::tui::shell::focus::{FocusTarget, OverlayKind};
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
    let mut effects = pipeline_compile::update(msg, state);
    effects.extend(tests_overlay::update(msg, state));
    effects.extend(summary_overlay::update(msg, state));
    effects.extend(pckg_overlay::update(msg, state));
    effects.extend(templates_overlay::update(msg, state));
    effects
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    let result = match state.focus {
        FocusTarget::Overlay(kind) => match kind {
            OverlayKind::Tests => tests_overlay::on_input(event, state),
            OverlayKind::Summary => summary_overlay::on_input(event, state),
            OverlayKind::Pckg => pckg_overlay::on_input(event, state),
            OverlayKind::Templates => templates_overlay::on_input(event, state),
        },
        FocusTarget::Base(_) => pipeline_compile::on_input(event, state),
    };
    if result == InputResult::Bubble {
        return pipeline_compile::on_input(event, state);
    }
    result
}

pub fn render(frame: &mut Frame, state: &mut ShellState) {
    super::render::draw_shell(frame, state);
}
