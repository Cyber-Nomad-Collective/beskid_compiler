//! Coordinator application for the Beskid shell (ratkit runtime on ratatui 0.30).

use crossterm::event::{KeyEvent, KeyEventState, MouseEvent as CrosstermMouseEvent};
use ratatui::Frame;

use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::kit::{
    self, CoordinatorAction, CoordinatorApp, CoordinatorEvent, KeyboardEvent, MouseEvent,
    RedrawSignal, Runner, RunnerConfig,
};
use crate::tui::message::ShellMessage;
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
}

impl BeskidShellApp {
    pub fn new(redraw_signal: RedrawSignal) -> Self {
        Self {
            state: ShellState::default(),
            redraw_signal,
            fx: ShellFx::default(),
            input_result: None,
            quit_requested: false,
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
        match event {
            CoordinatorEvent::Keyboard(keyboard) if !keyboard.is_key_down() => {
                Ok(CoordinatorAction::Continue)
            }
            CoordinatorEvent::Keyboard(keyboard) => {
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
