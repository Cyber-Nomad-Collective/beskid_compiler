use ratatui::layout::Rect;

use super::HiShellApp;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::realm::shell_event::{ShellOutcome, ShellRealmEvent};
use crate::tui::views;

impl HiShellApp {
    pub(crate) fn handle_shell_event(&mut self, event: ShellRealmEvent) -> ShellOutcome {
        self.drain_messages();

        if let ShellRealmEvent::Input(InputEvent::Mouse(mouse)) = &event {
            let action = self.handle_mouse(mouse, self.last_frame_area());
            if action != ShellOutcome::Continue {
                return action;
            }
        }

        if let Some(outcome) = self.handle_modal_input(&event) {
            return outcome;
        }

        match event {
            ShellRealmEvent::Input(InputEvent::Key(key)) => {
                if let Some(action) = self.handle_global_key(key) {
                    return action;
                }
                if let Some(action) = self.route_widget_input(key) {
                    return action;
                }
                let result = views::on_input(&InputEvent::Key(key), &mut self.shell_state);
                match result {
                    InputResult::Quit => {
                        self.quit_requested = true;
                        ShellOutcome::Quit
                    }
                    InputResult::CloseOverlay => {
                        self.shell_state.close_focused_overlay();
                        ShellOutcome::Redraw
                    }
                    _ => ShellOutcome::Redraw,
                }
            }
            ShellRealmEvent::Tick => {
                let changed = self.drain_messages();
                if changed || self.shell_state.pipeline_active() {
                    ShellOutcome::Redraw
                } else {
                    ShellOutcome::Continue
                }
            }
            ShellRealmEvent::Resize { width, height } => {
                self.set_frame_area(Rect { x: 0, y: 0, width, height });
                ShellOutcome::Redraw
            }
            ShellRealmEvent::Input(InputEvent::Mouse(mouse)) => {
                if let Some(action) = self.route_widget_input_mouse(mouse) {
                    return action;
                }
                let result = views::on_input(&InputEvent::Mouse(mouse), &mut self.shell_state);
                match result {
                    InputResult::Quit => {
                        self.quit_requested = true;
                        ShellOutcome::Quit
                    }
                    InputResult::CloseOverlay => {
                        self.shell_state.close_focused_overlay();
                        ShellOutcome::Redraw
                    }
                    _ => ShellOutcome::Redraw,
                }
            }
        }
    }
}
