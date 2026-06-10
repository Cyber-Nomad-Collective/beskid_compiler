use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::shell::primitives::Hotkey;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::title_line;
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
        let [title_area, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)])
            .areas(area);
        frame.render_widget(Paragraph::new(title_line("Welcome")), title_area);

        let scope_hint = match ctx.scope {
            ShellScope::User => "Open a project or workspace via the command palette (ctx.open_project / ctx.open_workspace).",
            ShellScope::Project { .. } => "Project scope — run analyze, test, and build from the palette or top menu.",
            ShellScope::Workspace { .. } => "Workspace scope — inspect graphs, dependencies, and targets from the Compiler menu.",
        };
        let lines = vec![
            Line::from(Span::styled(
                "Beskid interactive shell",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(scope_hint),
            Line::from(""),
            Line::from(Span::styled("F10", Style::default().fg(Color::Cyan))),
            Line::from("  Top menu — pages and tools"),
            Line::from(Span::styled("Ctrl+P / :", Style::default().fg(Color::Cyan))),
            Line::from("  Command palette"),
            Line::from(Span::styled("?", Style::default().fg(Color::Cyan))),
            Line::from("  Shortcut reference"),
        ];
        frame.render_widget(Paragraph::new(lines), body);
    }
}
