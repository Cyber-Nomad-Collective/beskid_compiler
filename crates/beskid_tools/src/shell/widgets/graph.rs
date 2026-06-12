use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::shell::primitives::Hotkey;

use crate::pipeline::tui::stage_focus::StageFocus;
use crate::pipeline::tui::widgets::draw_pipeline_tree;
use crate::shell::catalog::ContextualCommand;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::title_line;
use crate::shell::platform_shortcuts;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};

pub struct GraphWidget;

impl BeskidWidget for GraphWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "graph.deps",
            title: "Dependency graph",
            icon: "◎",
        }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn contextual_commands(&self, ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> {
        match ctx.scope {
            ShellScope::User => Vec::new(),
            _ => vec![ContextualCommand {
                id: "ctx.graph",
                name: "Dependency graph",
                description: "Open dependency graph view",
                icon: "◎",
                args_hint: None,
                widget_id: Some("graph.deps"),
            }],
        }
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        let [title_area, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)])
            .areas(area);
        frame.render_widget(
            Paragraph::new(title_line("Dependency graph")),
            title_area,
        );

        if ctx.scope.is_user() {
            frame.render_widget(Paragraph::new(ShellScope::no_project_lines()), body);
            return;
        }

        let scope_label = ctx.scope.label();
        let phase_count = ctx.shell_state.tree_nodes.len();
        let lines = vec![
            Line::from(vec![
                Span::styled("Scope ", Style::default().fg(Color::DarkGray)),
                Span::styled(scope_label, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            if phase_count > 0 {
                Line::from(format!(
                    "Pipeline tree has {phase_count} nodes — run `beskid graph` for the interactive graph TUI."
                ))
            } else {
                Line::from("Run `graph` from the command palette to explore workspace dependencies.")
            },
            Line::from(""),
            Line::from(Span::styled(
                format!("{} → graph", platform_shortcuts::palette_hint()),
                Style::default().fg(Color::DarkGray),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), body);
    }
}

pub struct GraphCompileWidget;

impl BeskidWidget for GraphCompileWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta {
            id: "graph.compile",
            title: "Compile graph",
            icon: "◈",
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
        frame.render_widget(
            Paragraph::new(title_line("Compile graph")),
            title_area,
        );

        if ctx.scope.is_user() {
            frame.render_widget(Paragraph::new(ShellScope::no_project_lines()), body);
            return;
        }

        if ctx.shell_state.tree_nodes.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "Run `build` or `test` to populate the compilation phase tree.",
                )
                .style(Style::default().fg(Color::DarkGray)),
                body,
            );
            return;
        }

        let focus = StageFocus::from_shell_state(ctx.shell_state);
        draw_pipeline_tree(
            frame,
            body,
            &ctx.shell_state.tree_nodes,
            &mut ctx.shell_state.tree_state,
            focus.title(),
        );
    }
}
