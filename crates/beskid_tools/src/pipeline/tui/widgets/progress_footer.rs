//! Dual pipeline progress gauges (stage + total).

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Gauge};

use super::super::model::PipelineProgress;

fn split_progress_footer(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .flex(Flex::SpaceBetween)
        .split(area);
    (chunks[0], chunks[1])
}

fn percent(done: u64, total: u64) -> u16 {
    let total = total.max(1);
    ((done.saturating_mul(100)) / total).min(100) as u16
}

pub fn draw_progress_footer(frame: &mut Frame, area: Rect, progress: &PipelineProgress) {
    let (stage_area, total_area) = split_progress_footer(area);

    let stage_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP)
                .title(format!(" {} ", progress.stage_label)),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .percent(percent(progress.stage_pos, progress.stage_len))
        .label(format!("{}/{}", progress.stage_pos, progress.stage_len));

    let total_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .title(format!(" {} ", progress.total_label)),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .percent(percent(progress.total_pos, progress.total_len))
        .label(format!("{}/{}", progress.total_pos, progress.total_len));

    frame.render_widget(stage_gauge, stage_area);
    frame.render_widget(total_gauge, total_area);
}
