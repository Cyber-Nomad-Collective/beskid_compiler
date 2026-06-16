use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::scope::ShellScope;
use crate::shell::shortcut_clicks::ShortcutClickAction;
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

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<crate::shell::primitives::Hotkey> {
        Vec::new()
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let palette_hint = ctx.key_bindings.palette_hint();
        let lines = match ctx.scope {
            ShellScope::User => vec![
                Line::from(Span::styled(
                    "Beskid interactive shell",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(format!(
                    "No project detected here. Use {palette_hint} → Open workspace / Open project."
                )),
                Line::from(""),
                Line::from(format!(
                    "Use {palette_hint} for pages, compiler commands, and tools."
                )),
            ],
            ShellScope::Project { manifest, .. } => scoped_lines(
                "Project",
                manifest
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("project"),
                "Run analyze, test, and build from the command palette.",
                &palette_hint,
            ),
            ShellScope::Workspace { manifest, .. } => scoped_lines(
                "Workspace",
                manifest
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("workspace"),
                "Inspect graphs, dependencies, and targets from the command palette.",
                &palette_hint,
            ),
        };

        frame.render_widget(Paragraph::new(lines), area);
        if ctx.scope.is_user() {
            ctx.shortcut_clicks
                .add_row(area, 4, ShortcutClickAction::OpenPalette);
        } else {
            ctx.shortcut_clicks
                .add_row(area, 3, ShortcutClickAction::OpenPalette);
        }
    }
}

fn scoped_lines(kind: &str, name: &str, hint: &str, palette_hint: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            format!("{kind}: {name}"),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(hint.to_string()),
        Line::from(""),
        Line::from(format!(
            "Use {palette_hint} for pages, compiler commands, and tools."
        )),
    ]
}
