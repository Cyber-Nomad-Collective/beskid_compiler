//! TachyonFX transitions scoped to overlay panels and compile chrome.

use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use tachyonfx::{fx, EffectManager, Interpolation};

use crate::tui::layout::{
    overlay_rect_for, OVERLAY_PCKG, OVERLAY_SUMMARY, OVERLAY_TEMPLATES, OVERLAY_TESTS,
};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::state::ShellState;

const FAST: (u32, Interpolation) = (180, Interpolation::SineOut);
const MEDIUM: (u32, Interpolation) = (280, Interpolation::QuadOut);

pub struct ShellFx {
    effects: EffectManager<()>,
    last_frame: Instant,
}

impl Default for ShellFx {
    fn default() -> Self {
        Self {
            effects: EffectManager::default(),
            last_frame: Instant::now(),
        }
    }
}

impl ShellFx {
    pub fn on_message(&mut self, msg: &ShellMessage, state: &ShellState) {
        match msg {
            ShellMessage::SetOverlayVisible { kind, visible: true } => {
                if let Some(area) = overlay_rect(state, *kind) {
                    self.queue_overlay_open(area);
                }
            }
            ShellMessage::SetOverlayVisible { kind, visible: false } => {
                if let Some(area) = overlay_rect(state, *kind) {
                    self.queue_overlay_close(area);
                }
            }
            ShellMessage::EnterProjectWizard => {
                if let Some(area) = state.layout_rects.templates_overlay {
                    self.queue_overlay_open(area);
                }
            }
            ShellMessage::CompileComplete => {
                let footer = state.layout_rects.footer;
                if footer.width > 0 && footer.height > 0 {
                    self.queue_compile_complete(footer);
                }
            }
            ShellMessage::PhaseEnd { .. } => {
                let stage = state.layout_rects.stage;
                if stage.width > 0 && stage.height > 0 {
                    self.queue_stage_pulse(stage);
                }
            }
            ShellMessage::BeginTests { .. } => {
                if let Some(area) = state.layout_rects.tests_overlay {
                    self.queue_overlay_open(area);
                }
            }
            ShellMessage::ShowTestReport { .. } | ShellMessage::StageSummary(_) => {
                if let Some(area) = state.layout_rects.summary_overlay {
                    self.queue_overlay_open(area);
                }
            }
            _ => {}
        }
    }

    pub fn process(&mut self, frame_area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        let elapsed = self.last_frame.elapsed();
        self.last_frame = Instant::now();
        let _ = self
            .effects
            .process_effects(elapsed.into(), buffer, frame_area);
    }

    fn queue_overlay_open(&mut self, area: Rect) {
        let backdrop = Style::default().bg(Color::Indexed(234));
        self.effects.add_effect(fx::coalesce_from(backdrop, FAST).with_area(area));
        self.effects.add_effect(
            fx::dissolve_to(Style::default().fg(Color::Cyan), FAST).with_area(area),
        );
    }

    fn queue_overlay_close(&mut self, area: Rect) {
        self.effects.add_effect(fx::dissolve(FAST).with_area(area));
    }

    fn queue_compile_complete(&mut self, area: Rect) {
        self.effects.add_effect(
            fx::sequence(&[
                fx::lighten_fg(0.35, MEDIUM).with_area(area),
                fx::fade_to_fg(Color::Reset, MEDIUM).with_area(area),
            ]),
        );
    }

    fn queue_stage_pulse(&mut self, area: Rect) {
        self.effects.add_effect(
            fx::dissolve_to(Style::default().fg(Color::DarkGray), (120, Interpolation::Linear))
                .with_area(area),
        );
    }
}

fn overlay_rect(state: &ShellState, kind: OverlayKind) -> Option<Rect> {
    if let Some(cached) = match kind {
        OverlayKind::Tests => state.layout_rects.tests_overlay,
        OverlayKind::Summary => state.layout_rects.summary_overlay,
        OverlayKind::Pckg => state.layout_rects.pckg_overlay,
        OverlayKind::Templates => state.layout_rects.templates_overlay,
    } {
        return Some(cached);
    }
    let terminal = terminal_area(state)?;
    Some(match kind {
        OverlayKind::Tests => overlay_rect_for(OVERLAY_TESTS, terminal),
        OverlayKind::Summary => overlay_rect_for(OVERLAY_SUMMARY, terminal),
        OverlayKind::Pckg => overlay_rect_for(OVERLAY_PCKG, terminal),
        OverlayKind::Templates => overlay_rect_for(OVERLAY_TEMPLATES, terminal),
    })
}

fn terminal_area(state: &ShellState) -> Option<Rect> {
    let header = state.layout_rects.header;
    if header.width == 0 {
        return None;
    }
    let footer = state.layout_rects.footer;
    Some(Rect {
        x: header.x,
        y: header.y,
        width: header.width,
        height: footer
            .y
            .saturating_add(footer.height)
            .saturating_sub(header.y),
    })
}
