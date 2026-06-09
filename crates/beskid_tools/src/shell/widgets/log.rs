use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratkit::services::hotkey_service::Hotkey;

use crate::pipeline::tui::widgets::draw_tabbed_log_panel;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct LogWidget;

impl BeskidWidget for LogWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.log",
            title: "Log",
            icon: "≡",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        if ctx.shell_state.shell_mode == crate::tui::shell::pane_state::ShellMode::Hi {
            frame.render_widget(
                Paragraph::new("Session log (build/analyze output appears when commands run)")
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::ALL).title(" Log ")),
                area,
            );
        } else {
            draw_tabbed_log_panel(
                frame,
                area,
                ctx.shell_state.log_tab,
                &mut ctx.shell_state.log_states,
            );
        }
    }
}

pub struct LogPanelWidget;

impl BeskidWidget for LogPanelWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "pipeline.log",
            title: "Build log",
            icon: "≡",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        draw_tabbed_log_panel(
            frame,
            area,
            ctx.shell_state.log_tab,
            &mut ctx.shell_state.log_states,
        );
    }
}
