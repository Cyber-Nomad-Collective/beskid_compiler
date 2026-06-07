//! Post-run test summary with pie chart and failure log panel.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Color;
use ratatui::widgets::{Block, Borders, Widget};
use tui_logger::TuiWidgetState;
use tui_piechart::{PieChart, PieSlice};

use super::logger_panel::{draw_log_panel, init_session_logger};
use super::test_table::{TestRow, TestRowState};

/// Outcome counts for the test run report.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestReportSummary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub filtered_out: usize,
}

impl TestReportSummary {
    pub fn total(&self) -> usize {
        self.passed + self.failed + self.skipped + self.filtered_out
    }
}

/// Push failure details into the tui-logger buffer for the report panel.
pub fn seed_failure_logs(rows: &[TestRow]) {
    for row in rows {
        if row.state == TestRowState::Failed {
            tracing::error!(target: "beskid.tools.test", name = row.qualified_name.as_str(), "FAIL");
        }
    }
}

pub fn init_test_logger() {
    init_session_logger();
}

pub fn draw_test_report(
    frame: &mut Frame,
    area: Rect,
    summary: TestReportSummary,
    title: &str,
    logger_state: &mut TuiWidgetState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[0]);

    draw_pie_chart(frame, body[0], summary, title);
    draw_failure_log(frame, body[1], logger_state);
    draw_totals_bar(frame, chunks[1], summary);
}

fn draw_pie_chart(frame: &mut Frame, area: Rect, summary: TestReportSummary, title: &str) {
    let total = summary.total().max(1) as f64;
    let mut slices = Vec::new();
    if summary.passed > 0 {
        slices.push(PieSlice::new(
            "pass",
            summary.passed as f64 * 100.0 / total,
            Color::Green,
        ));
    }
    if summary.failed > 0 {
        slices.push(PieSlice::new(
            "fail",
            summary.failed as f64 * 100.0 / total,
            Color::Red,
        ));
    }
    if summary.skipped > 0 {
        slices.push(PieSlice::new(
            "skip",
            summary.skipped as f64 * 100.0 / total,
            Color::Blue,
        ));
    }
    if summary.filtered_out > 0 {
        slices.push(PieSlice::new(
            "filt",
            summary.filtered_out as f64 * 100.0 / total,
            Color::DarkGray,
        ));
    }
    if slices.is_empty() {
        slices.push(PieSlice::new("empty", 100.0, Color::DarkGray));
    }
    let chart = PieChart::new(slices)
        .show_legend(true)
        .show_percentages(true)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} ")),
        );
    chart.render(area, frame.buffer_mut());
}

fn draw_failure_log(frame: &mut Frame, area: Rect, logger_state: &mut TuiWidgetState) {
    draw_log_panel(frame, area, "Failures", logger_state);
}

fn draw_totals_bar(frame: &mut Frame, area: Rect, summary: TestReportSummary) {
    let line = format!(
        "passed={} failed={} skipped={} filtered={}",
        summary.passed, summary.failed, summary.skipped, summary.filtered_out
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Result ");
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());
    ratatui::widgets::Paragraph::new(line).render(inner, frame.buffer_mut());
}
