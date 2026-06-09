//! Generic command summary: pie chart, stats table, headline.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Widget};
use tui_piechart::{PieChart, PieSlice};

use super::super::model::CommandSummary;

/// Secondary pane in the unified shell (chart or stats table).
pub fn draw_summary_chart_panel(frame: &mut Frame, area: Rect, summary: &CommandSummary) {
    if !summary.slices.is_empty() {
        draw_pie_chart(frame, area, summary);
    } else {
        draw_stats_table(frame, area, summary);
    }
}

/// Footer strip for summary / report modes.
pub fn draw_summary_headline_footer(frame: &mut Frame, area: Rect, summary: &CommandSummary) {
    let headline = Paragraph::new(summary.headline.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", summary.title)),
    );
    frame.render_widget(headline, area);
}

fn draw_pie_chart(frame: &mut Frame, area: Rect, summary: &CommandSummary) {
    let slices: Vec<PieSlice> = summary
        .slices
        .iter()
        .map(|slice| PieSlice::new(slice.label.as_str(), slice.percent, slice.color))
        .collect();
    let chart = PieChart::new(slices)
        .show_legend(true)
        .show_percentages(true)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", summary.title)),
        );
    chart.render(area, frame.buffer_mut());
}

fn draw_stats_table(frame: &mut Frame, area: Rect, summary: &CommandSummary) {
    if summary.stats.is_empty() {
        let placeholder = Paragraph::new(summary.headline.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", summary.title)),
        );
        frame.render_widget(placeholder, area);
        return;
    }
    let rows: Vec<Row> = summary
        .stats
        .iter()
        .map(|stat| {
            let style = stat
                .color
                .map(|color| Style::default().fg(color))
                .unwrap_or_default();
            Row::new(vec![
                Cell::from(stat.label.as_str()).style(style.add_modifier(Modifier::BOLD)),
                Cell::from(stat.value.as_str()).style(style),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [ratatui::layout::Constraint::Length(14), ratatui::layout::Constraint::Min(4)],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", summary.title)),
    );
    frame.render_widget(table, area);
}
