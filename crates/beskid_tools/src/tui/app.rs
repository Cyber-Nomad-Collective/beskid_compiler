//! Pipeline shell state and tuirealm event handling.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Frame;

use crate::shell::catalog::builtin_contextual_commands;
use crate::shell::palette::{self, CommandPaletteState, PaletteAction};
use crate::shell::scope::ShellScope;
use crate::shell::widget::ShellAction;
use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::realm::shell_event::{ShellOutcome, ShellRealmEvent};
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::state::ShellState;
use crate::tui::shell_fx::ShellFx;
use crate::tui::signals::RedrawSignal;
use crate::tui::views;

/// Pipeline compile shell (hosted by tuirealm [`PipelineShellComponent`](crate::tui::realm::PipelineShellComponent)).
pub struct BeskidShellApp {
    pub state: ShellState,
    pub redraw_signal: RedrawSignal,
    fx: ShellFx,
    pub input_result: Option<InputResult>,
    pub quit_requested: bool,
    palette: CommandPaletteState,
    scope: ShellScope,
}

impl BeskidShellApp {
    pub fn new(redraw_signal: RedrawSignal) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            state: ShellState::default(),
            redraw_signal,
            fx: ShellFx::default(),
            input_result: None,
            quit_requested: false,
            palette: CommandPaletteState::default(),
            scope: ShellScope::resolve(&cwd),
        }
    }

    pub fn handle_shell_event(&mut self, event: ShellRealmEvent) -> ShellOutcome {
        if self.palette.visible {
            if let ShellRealmEvent::Input(InputEvent::Key(key)) = event {
                let action = self.palette.handle_key(key);
                self.handle_palette_action(action);
            }
            return ShellOutcome::Redraw;
        }

        match event {
            ShellRealmEvent::Input(InputEvent::Key(key)) => {
                if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p'))
                    || key.code == KeyCode::Char(':')
                {
                    self.open_palette();
                    return ShellOutcome::Redraw;
                }
                let result = views::on_input(&InputEvent::Key(key), &mut self.state);
                self.input_result = Some(result);
                if result == InputResult::Quit {
                    self.quit_requested = true;
                    return ShellOutcome::Quit;
                }
                ShellOutcome::Redraw
            }
            ShellRealmEvent::Input(InputEvent::Mouse(mouse)) => {
                let result = views::on_input(&InputEvent::Mouse(mouse), &mut self.state);
                self.input_result = Some(result);
                if result == InputResult::Quit {
                    self.quit_requested = true;
                    return ShellOutcome::Quit;
                }
                ShellOutcome::Redraw
            }
            ShellRealmEvent::Tick | ShellRealmEvent::Resize { .. } => ShellOutcome::Redraw,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        views::render(frame, &mut self.state);
        let area = frame.area();
        if self.palette.visible {
            self.palette.render(
                area,
                frame,
                &crate::shell::key_bindings::ShortcutBindings::platform_defaults().palette_hint(),
            );
        }
        self.fx.process(area, frame.buffer_mut());
    }

    pub fn apply_message(&mut self, msg: &ShellMessage) -> Vec<ShellEffect> {
        let effects = views::update(msg, &mut self.state);
        self.fx.on_message(msg, &self.state);
        effects
    }

    pub fn take_input_result(&mut self) -> Option<InputResult> {
        self.input_result.take()
    }

    fn open_palette(&mut self) {
        self.palette.open(builtin_contextual_commands(&self.scope));
    }

    fn handle_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::None | PaletteAction::Redraw => {}
            PaletteAction::Close => {}
            PaletteAction::Execute(item, _params) => {
                self.palette.close();
                self.apply_shell_action(palette::contextual_to_shell_action(&item));
            }
        }
    }

    fn apply_shell_action(&mut self, action: ShellAction) {
        match action {
            ShellAction::OpenPalette => self.open_palette(),
            ShellAction::RunContextual("ctx.palette") => self.open_palette(),
            ShellAction::OpenOverlay(widget_id) => match widget_id {
                "pckg.browser" => {
                    self.state.set_overlay_visible(OverlayKind::Pckg, true);
                    self.state.focus_overlay(OverlayKind::Pckg);
                    if !self.state.pckg.catalog_loaded {
                        self.state.pckg.pending_catalog_refresh = true;
                    }
                }
                "tests.runner" => {
                    self.state.set_overlay_visible(OverlayKind::Tests, true);
                    self.state.focus_overlay(OverlayKind::Tests);
                }
                "templates.picker" => {
                    self.state.set_overlay_visible(OverlayKind::Templates, true);
                    self.state.focus_overlay(OverlayKind::Templates);
                    if !self.state.templates.catalog_loaded {
                        self.state.templates.pending_catalog_refresh = true;
                    }
                }
                "graph.deps" => {
                    self.state.set_overlay_visible(OverlayKind::Graph, true);
                    self.state.focus_overlay(OverlayKind::Graph);
                }
                _ => {}
            },
            ShellAction::Quit => self.quit_requested = true,
            ShellAction::Redraw | ShellAction::None | ShellAction::RunContextual(_) => {}
        }
    }
}
