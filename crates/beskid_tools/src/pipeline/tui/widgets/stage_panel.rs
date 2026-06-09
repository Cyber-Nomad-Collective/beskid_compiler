//! Primary pane: stage focus blurb and live work unit (gauges live in the footer only).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::super::stage_focus::StageFocus;
use super::super::test_table::TestRowState;
use super::spinner::draw_stage_bar_spinner;
use crate::tui::shell::focus::{FocusTarget, OverlayKind};
use crate::tui::shell::state::{NavTarget, ShellState};

pub fn draw_stage_panel(frame: &mut Frame, area: Rect, state: &ShellState, focus: StageFocus) {
    match state.focus {
        FocusTarget::Overlay(OverlayKind::Tests) => draw_test_stage(frame, area, state),
        FocusTarget::Overlay(OverlayKind::Summary) => draw_summary_stage(frame, area, state),
        _ => draw_pipeline_stage(frame, area, state, focus),
    }
}

fn draw_pipeline_stage(frame: &mut Frame, area: Rect, state: &ShellState, focus: StageFocus) {
    let title = match focus {
        StageFocus::Workspace => "Workspace",
        StageFocus::FrontEnd => "Front end",
        StageFocus::Semantic => "Semantic analysis",
        StageFocus::LowerCodegen => "Lowering & codegen",
        _ => "Stage",
    };

    let mut lines = Vec::new();
    if state.compile_complete
        && state.awaiting_nav == Some(NavTarget::Tests)
        && state.tests_loaded
    {
        lines.push(Line::from(vec![
            Span::styled(
                "Compile finished — press Space to run tests",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
    }
    if let Some(work) = state.last_work_unit.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("Now ", Style::default().fg(Color::DarkGray)),
            Span::styled(work, Style::default().add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(""));
    } else if !state.pipeline.stage_label.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                state.pipeline.stage_label.as_str(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
    }
    push_focus_description(&mut lines, focus);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "));
    if state.show_spinner() {
        let [spinner_area, text_area] =
            ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Length(1),
                ratatui::layout::Constraint::Min(2),
            ])
            .areas(area);
        draw_stage_bar_spinner(frame, spinner_area, state.tick);
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
            text_area,
        );
        return;
    }

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
        area,
    );
}

fn draw_test_stage(frame: &mut Frame, area: Rect, state: &ShellState) {
    let title = state.test_title.as_deref().unwrap_or("Tests");
    let running = state
        .test_rows
        .iter()
        .find(|row| row.state == TestRowState::Running);
    let mut lines = if let Some(row) = running {
        vec![Line::from(vec![
            Span::styled("Running ", Style::default().fg(Color::Yellow)),
            Span::raw(row.qualified_name.as_str()),
        ])]
    } else {
        vec![Line::from(Span::styled(
            "Waiting for next test…",
            Style::default().fg(Color::DarkGray),
        ))]
    };
    push_focus_description(&mut lines, StageFocus::Tests);
    if !state.test_rows.is_empty() {
        let passed = state
            .test_rows
            .iter()
            .filter(|row| row.state == TestRowState::Passed)
            .count();
        let failed = state
            .test_rows
            .iter()
            .filter(|row| row.state == TestRowState::Failed)
            .count();
        let pending = state
            .test_rows
            .iter()
            .filter(|row| {
                matches!(
                    row.state,
                    TestRowState::Pending | TestRowState::Running
                )
            })
            .count();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Pass ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                passed.to_string(),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled("Fail ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                failed.to_string(),
                Style::default().fg(if failed > 0 {
                    Color::Red
                } else {
                    Color::DarkGray
                }),
            ),
            Span::raw("  "),
            Span::styled("Pending ", Style::default().fg(Color::DarkGray)),
            Span::raw(pending.to_string()),
        ]));
    }
    let widget = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} ")),
        );
    frame.render_widget(widget, area);
}

fn draw_summary_stage(frame: &mut Frame, area: Rect, state: &ShellState) {
    let summary = &state.command_summary;
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                summary.title.as_str(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(summary.headline.as_str()),
    ];
    push_focus_description(&mut lines, StageFocus::Summary);
    let widget = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Result "),
        );
    frame.render_widget(widget, area);
}

fn push_focus_description(lines: &mut Vec<Line<'_>>, focus: StageFocus) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        focus.description(),
        Style::default().fg(Color::DarkGray),
    )));
}
