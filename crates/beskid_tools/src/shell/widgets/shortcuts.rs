use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::shell::primitives::Hotkey;

use crate::shell::catalog::builtin_contextual_commands;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::title_line;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct ShortcutsWidget;

impl BeskidWidget for ShortcutsWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "shell.shortcuts",
            title: "Shortcuts",
            icon: "?",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let mut lines = vec![
            Line::from(Span::styled("Global", Style::default().fg(Color::Yellow))),
            Line::from("  Ctrl+P / :  command palette"),
            Line::from("  ?           shortcut help"),
            Line::from("  q           quit"),
            Line::from(""),
        ];
        for cmd in builtin_contextual_commands(ctx.scope) {
            if let crate::shell::catalog::CommandItem::Contextual(c) = cmd {
                lines.push(Line::from(format!("  {}  {}", c.icon, c.name)));
            }
        }
        let [title_area, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)])
            .areas(area);
        frame.render_widget(Paragraph::new(title_line("Shortcuts")), title_area);
        frame.render_widget(Paragraph::new(lines), body);
    }
}
