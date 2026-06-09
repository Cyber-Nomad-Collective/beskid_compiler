//! Coordinator application for the Beskid shell (ratkit runtime on ratatui 0.30).

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers, MouseEvent as CrosstermMouseEvent};
use ratatui::Frame;

use crate::shell::catalog::{builtin_cli_commands, builtin_contextual_commands};
use crate::shell::palette::{self, CommandPaletteState, PaletteAction};
use crate::shell::scope::ShellScope;
use crate::shell::widget::ShellAction;
use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::kit::{
    self, CoordinatorAction, CoordinatorApp, CoordinatorEvent, KeyboardEvent, MouseEvent,
    RedrawSignal, Runner, RunnerConfig,
};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::state::ShellState;
use crate::tui::shell_fx::ShellFx;
use crate::tui::views;

/// Shell state + ratkit runner + tachyonfx transitions.
pub struct BeskidShellApp {
    pub state: ShellState,
    pub redraw_signal: RedrawSignal,
    fx: ShellFx,
    pub input_result: Option<InputResult>,
    pub quit_requested: bool,
    palette: CommandPaletteState,
    scope: ShellScope,
    beskid_exe: PathBuf,
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
            beskid_exe: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("beskid")),
        }
    }

    fn open_palette(&mut self) {
        let mut items = builtin_cli_commands();
        items.extend(builtin_contextual_commands(&self.scope));
        // compile pipeline shell: layout editor not active
        self.palette.open(items);
    }

    fn handle_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::None | PaletteAction::Redraw => {}
            PaletteAction::Close => {}
            PaletteAction::Execute(item, params) => {
                self.palette.close();
                if item.kind() == crate::shell::catalog::CommandKind::Cli {
                    let _ = palette::execute_cli_command(
                        &self.beskid_exe,
                        &item,
                        &params,
                        &self.scope,
                    );
                } else {
                    self.apply_shell_action(palette::contextual_to_shell_action(&item));
                }
            }
        }
    }

    fn apply_shell_action(&mut self, action: ShellAction) {
        match action {
            ShellAction::OpenPalette => self.open_palette(),
            ShellAction::RunContextual(id) if id == "ctx.palette" => self.open_palette(),
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
                    let _ = palette::execute_cli_command(
                        &self.beskid_exe,
                        &crate::shell::catalog::CommandItem::Cli(
                            crate::shell::catalog::CliCommandDef {
                                id: "graph",
                                name: "graph",
                                description: "graph",
                                icon: "◎",
                                argv_prefix: &["graph", "--tui"],
                                args_hint: "",
                            },
                        ),
                        "",
                        &self.scope,
                    );
                }
                _ => {}
            },
            ShellAction::Quit => self.quit_requested = true,
            ShellAction::Redraw | ShellAction::None | ShellAction::RunContextual(_) => {}
        }
    }

    pub fn apply_message(&mut self, msg: &ShellMessage) -> Vec<ShellEffect> {
        let effects = views::update(msg, &mut self.state);
        self.fx.on_message(msg, &self.state);
        effects
    }

    pub fn take_input_result(&mut self) -> Option<InputResult> {
        self.input_result.take()
    }

    fn keyboard_to_input(keyboard: &KeyboardEvent) -> InputEvent {
        InputEvent::Key(KeyEvent {
            code: keyboard.key_code,
            modifiers: keyboard.modifiers,
            kind: keyboard.kind,
            state: KeyEventState::empty(),
        })
    }

    fn mouse_to_input(mouse: &MouseEvent) -> InputEvent {
        InputEvent::Mouse(CrosstermMouseEvent {
            kind: mouse.kind,
            column: mouse.column,
            row: mouse.row,
            modifiers: mouse.modifiers,
        })
    }
}

impl CoordinatorApp for BeskidShellApp {
    fn on_event(&mut self, event: CoordinatorEvent) -> kit::LayoutResult<CoordinatorAction> {
        if self.palette.visible {
            if let CoordinatorEvent::Keyboard(keyboard) = &event {
                let key = KeyEvent {
                    code: keyboard.key_code,
                    modifiers: keyboard.modifiers,
                    kind: keyboard.kind,
                    state: KeyEventState::empty(),
                };
                let action = self.palette.handle_key(key);
                self.handle_palette_action(action);
                return Ok(CoordinatorAction::Redraw);
            }
            return Ok(CoordinatorAction::Redraw);
        }

        match event {
            CoordinatorEvent::Keyboard(keyboard) if !keyboard.is_key_down() => {
                Ok(CoordinatorAction::Continue)
            }
            CoordinatorEvent::Keyboard(keyboard) => {
                let key = KeyEvent {
                    code: keyboard.key_code,
                    modifiers: keyboard.modifiers,
                    kind: keyboard.kind,
                    state: KeyEventState::empty(),
                };
                if (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p'))
                    || key.code == KeyCode::Char(':')
                {
                    self.open_palette();
                    return Ok(CoordinatorAction::Redraw);
                }
                let result = views::on_input(&Self::keyboard_to_input(&keyboard), &mut self.state);
                self.input_result = Some(result);
                if result == InputResult::Quit {
                    self.quit_requested = true;
                    return Ok(CoordinatorAction::Quit);
                }
                Ok(CoordinatorAction::Redraw)
            }
            CoordinatorEvent::Mouse(mouse) => {
                let result = views::on_input(&Self::mouse_to_input(&mouse), &mut self.state);
                self.input_result = Some(result);
                if result == InputResult::Quit {
                    self.quit_requested = true;
                    return Ok(CoordinatorAction::Quit);
                }
                Ok(CoordinatorAction::Redraw)
            }
            CoordinatorEvent::Tick(_) => Ok(CoordinatorAction::Redraw),
            CoordinatorEvent::Resize(_) => Ok(CoordinatorAction::Redraw),
            _ => Ok(CoordinatorAction::Continue),
        }
    }

    fn on_draw(&mut self, frame: &mut Frame) {
        views::render(frame, &mut self.state);
        let area = frame.area();
        if self.palette.visible {
            self.palette.render(area, frame);
        }
        self.fx.process(area, frame.buffer_mut());
    }
}

pub fn new_runner(app: BeskidShellApp) -> Runner<BeskidShellApp> {
    Runner::new(app).with_config(RunnerConfig {
        tick_rate: std::time::Duration::from_millis(80),
        ..RunnerConfig::default()
    })
}

pub use kit::{map_runner_action, runner_event_from_crossterm, tick_event, ResizeEvent, RunnerEvent};
