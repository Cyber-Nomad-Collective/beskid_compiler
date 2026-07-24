use crate::shell::primitives::Hotkey;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::pipeline::tui::SeverityCounts;
use crate::shell::catalog::ContextualCommand;
use crate::shell::context::WidgetContext;
use crate::shell::input::ShellInput;
use crate::shell::panel_style::title_line;
use crate::shell::scope::ShellScope;
use crate::shell::widget::{BeskidWidget, ShellAction, WidgetMeta};
use crate::tui::shell::focus::OverlayKind;

pub struct AnalysisWidget;

impl BeskidWidget for AnalysisWidget {
    fn meta(&self) -> WidgetMeta {
        WidgetMeta { id: "analysis.diagnostics", title: "Build Report", icon: "\u{25c7}" }
    }

    fn hotkeys(&self, _ctx: &WidgetContext<'_>) -> Vec<Hotkey> {
        Vec::new()
    }

    fn contextual_commands(&self, ctx: &WidgetContext<'_>) -> Vec<ContextualCommand> {
        match ctx.scope {
            ShellScope::User => Vec::new(),
            _ => vec![ContextualCommand {
                id: "ctx.analyze",
                name: "Analyze",
                description: "Run semantic analysis in scope",
                icon: "\u{25c7}",
                args_hint: None,
                widget_id: Some("analysis.diagnostics"),
            }],
        }
    }

    fn on_input(&mut self, _event: &ShellInput, _ctx: &mut WidgetContext<'_>) -> ShellAction {
        ShellAction::None
    }

    fn render(&self, area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
        draw_build_report(area, frame, ctx);
    }
}

pub fn draw_build_report(area: Rect, frame: &mut Frame, ctx: &mut WidgetContext<'_>) {
    let [title_area, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    frame.render_widget(Paragraph::new(title_line("Build Report")), title_area);

    if ctx.scope.is_user() {
        frame.render_widget(Paragraph::new(ShellScope::no_project_lines(&ctx.key_bindings.palette_hint())), body);
        return;
    }

    let mut lines = Vec::new();
    let state = &ctx.shell_state;

    if state.compile_complete {
        lines.push(Line::from(Span::styled("\u{2714} Build complete", Style::default().fg(Color::Green))));
        lines.push(Line::from(""));

        // Severity summary
        if let Some(counts) = severity_counts_from_summary(&state.command_summary) {
            lines.push(Line::from(vec![
                Span::styled("Errors: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    counts.errors.to_string(),
                    Style::default().fg(if counts.errors > 0 { Color::Red } else { Color::Green }),
                ),
                Span::raw("   "),
                Span::styled("Warnings: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    counts.warnings.to_string(),
                    Style::default().fg(if counts.warnings > 0 { Color::Yellow } else { Color::Green }),
                ),
                Span::raw("   "),
                Span::styled("Notes: ", Style::default().fg(Color::DarkGray)),
                Span::styled(counts.notes.to_string(), Style::default().fg(Color::Blue)),
            ]));
        }

        // Diagnostic message body
        if !state.command_summary.headline.is_empty() {
            lines.push(Line::from(""));
            for line_text in state.command_summary.headline.lines() {
                let trimmed = line_text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let style = if trimmed.starts_with("error") || trimmed.starts_with("Error") {
                    Style::default().fg(Color::Red)
                } else if trimmed.starts_with("warning") || trimmed.starts_with("Warning") {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(trimmed, style)));
            }
        }

        // Test results
        if !state.test_rows.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Tests:", Style::default().fg(Color::DarkGray))));
            for row in state.test_rows.iter().take(20) {
                let (icon, color) = match row.state {
                    crate::pipeline::tui::TestRowState::Passed => ("✔", Color::Green),
                    crate::pipeline::tui::TestRowState::Failed => ("✘", Color::Red),
                    crate::pipeline::tui::TestRowState::Skipped => ("◦", Color::Blue),
                    crate::pipeline::tui::TestRowState::Pending => ("○", Color::DarkGray),
                    crate::pipeline::tui::TestRowState::Running => ("▶", Color::Yellow),
                    crate::pipeline::tui::TestRowState::FilteredOut => continue,
                };
                let suffix = match &row.failure_detail {
                    Some(reason) if !reason.is_empty() => {
                        format!(" — {}", reason.lines().next().unwrap_or(""))
                    }
                    _ => String::new(),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  {icon} "), Style::default().fg(color)),
                    Span::styled(&row.qualified_name, Style::default().fg(color)),
                    Span::styled(suffix, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    } else {
        lines.push(Line::from("○ Idle — run a build or test to see results"));
        lines.push(Line::from(""));
        lines
            .push(Line::from(Span::styled("Open the command palette to start.", Style::default().fg(Color::DarkGray))));
    }

    frame.render_widget(Paragraph::new(lines), body);
}

fn severity_counts_from_summary(summary: &crate::pipeline::tui::CommandSummary) -> Option<SeverityCounts> {
    if summary.stats.is_empty() {
        return None;
    }
    let mut counts = SeverityCounts::default();
    let mut matched = false;
    for stat in &summary.stats {
        match stat.label.as_str() {
            "errors" => {
                counts.errors = stat.value.parse().unwrap_or(0);
                matched = true;
            }
            "warnings" => {
                counts.warnings = stat.value.parse().unwrap_or(0);
                matched = true;
            }
            "notes" => {
                counts.notes = stat.value.parse().unwrap_or(0);
                matched = true;
            }
            _ => {}
        }
    }
    if matched { Some(counts) } else { None }
}

pub fn open_analysis(ctx: &mut WidgetContext<'_>) {
    ctx.shell_state.set_overlay_visible(OverlayKind::Analysis, true);
    ctx.shell_state.focus_overlay(OverlayKind::Analysis);
}
