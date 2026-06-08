//! Primary pane: stage-appropriate headline, progress, and work-unit detail.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Wrap};

use super::super::model::{Mode, Model};
use super::super::stage_focus::StageFocus;
use super::super::test_table::TestRowState;

fn percent(done: u64, total: u64) -> u16 {
    let total = total.max(1);
    ((done.saturating_mul(100)) / total).min(100) as u16
}

pub fn draw_stage_panel(frame: &mut Frame, area: Rect, model: &Model, focus: StageFocus) {
    match model.mode {
        Mode::Pipeline => draw_pipeline_stage(frame, area, model, focus),
        Mode::Tests => draw_test_stage(frame, area, model),
        Mode::Report | Mode::Summary => draw_summary_stage(frame, area, model),
    }
}

fn draw_pipeline_stage(frame: &mut Frame, area: Rect, model: &Model, focus: StageFocus) {
    let (detail_area, gauge_area) = split_detail_gauge(area);
    let title = match focus {
        StageFocus::Workspace => "Workspace progress",
        StageFocus::FrontEnd => "Parse & macros",
        StageFocus::Semantic => "Semantic pass",
        StageFocus::LowerCodegen => "Codegen",
        _ => "Stage",
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                model.pipeline.stage_label.as_str(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    if let Some(work) = model.last_work_unit.as_deref() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Work ", Style::default().fg(Color::DarkGray)),
            Span::raw(work),
        ]));
    }
    push_focus_description(&mut lines, focus);

    let detail = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} ")),
        );
    frame.render_widget(detail, detail_area);

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .title(" Stage "),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .percent(percent(
            model.pipeline.stage_pos,
            model.pipeline.stage_len,
        ))
        .label(format!(
            "{}/{}",
            model.pipeline.stage_pos, model.pipeline.stage_len
        ));
    frame.render_widget(gauge, gauge_area);
}

fn draw_test_stage(frame: &mut Frame, area: Rect, model: &Model) {
    let title = model.test_title.as_deref().unwrap_or("Tests");
    let running = model
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
    if !model.test_rows.is_empty() {
        let passed = model
            .test_rows
            .iter()
            .filter(|row| row.state == TestRowState::Passed)
            .count();
        let failed = model
            .test_rows
            .iter()
            .filter(|row| row.state == TestRowState::Failed)
            .count();
        let pending = model
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

fn draw_summary_stage(frame: &mut Frame, area: Rect, model: &Model) {
    let summary = &model.command_summary;
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

fn split_detail_gauge(area: Rect) -> (Rect, Rect) {
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Min(3),
            ratatui::layout::Constraint::Length(3),
        ])
        .flex(ratatui::layout::Flex::Legacy)
        .split(area);
    (chunks[0], chunks[1])
}
