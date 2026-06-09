use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratkit::services::hotkey_service::Hotkey;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct HiWelcomeWidget;

impl BeskidWidget for HiWelcomeWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "hi.welcome",
            title: "Welcome",
            icon: "◇",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let hint = match ctx.scope {
            ShellScope::User => "Open a Beskid project or workspace directory, or use the command palette to run CLI commands.",
            ShellScope::Project { .. } => "Project scope: run analyze, test, build, or browse packages from the palette.",
            ShellScope::Workspace { .. } => "Workspace scope: inspect graphs, manage dependencies, and run targets.",
        };
        let lines = vec![
            Line::from(Span::styled(
                "Beskid Hi",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(hint),
            Line::from(""),
            Line::from(Span::styled(
                "Ctrl+P or : — command palette",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Welcome ")),
            area,
        );
    }
}
