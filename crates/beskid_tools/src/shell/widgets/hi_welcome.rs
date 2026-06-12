use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::shell::primitives::Hotkey;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::title_line;
use crate::shell::platform_shortcuts;
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

        let user_scope_hint = format!(
            "No project detected here. Use {} → Open workspace / Open project, or launch from a directory with a `.bws` / `.bproj`.",
            platform_shortcuts::palette_hint()
        );
        let scope_hint = match ctx.scope {
            ShellScope::User => user_scope_hint.as_str(),
            ShellScope::Project { manifest, .. } => {
                return render_scoped_welcome(
                    frame,
                    title_area,
                    body,
                    "Project",
                    manifest.file_stem().and_then(|s| s.to_str()).unwrap_or("project"),
                    "Run analyze, test, and build from the palette or Compiler menu.",
                );
            }
            ShellScope::Workspace { manifest, .. } => {
                return render_scoped_welcome(
                    frame,
                    title_area,
                    body,
                    "Workspace",
                    manifest.file_stem().and_then(|s| s.to_str()).unwrap_or("workspace"),
                    "Inspect graphs, dependencies, and targets from the Compiler menu.",
                );
            }
        };
        let lines = vec![
            Line::from(Span::styled(
                "Beskid interactive shell",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(scope_hint),
            Line::from(""),
            Line::from(Span::styled(
                platform_shortcuts::menu_hint(),
                Style::default().fg(Color::Cyan),
            )),
            Line::from("  Top menu — pages and tools"),
            Line::from(Span::styled(
                platform_shortcuts::palette_hint(),
                Style::default().fg(Color::Cyan),
            )),
            Line::from("  Command palette"),
            Line::from(Span::styled("?", Style::default().fg(Color::Cyan))),
            Line::from("  Shortcut reference"),
        ];
        frame.render_widget(Paragraph::new(lines), body);
    }
}

fn render_scoped_welcome(
    frame: &mut Frame,
    _title_area: Rect,
    body: Rect,
    kind: &str,
    name: &str,
    hint: &str,
) {
    let lines = vec![
        Line::from(Span::styled(
            format!("{kind}: {name}"),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(hint),
        Line::from(""),
        Line::from(Span::styled(
            platform_shortcuts::menu_hint(),
            Style::default().fg(Color::Cyan),
        )),
        Line::from("  Top menu — pages and tools"),
        Line::from(Span::styled(
            platform_shortcuts::palette_hint(),
            Style::default().fg(Color::Cyan),
        )),
        Line::from("  Command palette"),
    ];
    frame.render_widget(Paragraph::new(lines), body);
}
