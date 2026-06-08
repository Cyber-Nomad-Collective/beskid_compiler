//! TEA view: render the model (no business logic).

use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::layout::{
    FOOTER_HEIGHT, TEST_LIST_PANEL_RATIO, TREE_PANEL_RATIO, split_main_footer, split_panels,
};
use super::model::{Mode, Model};
use super::test_table::TestRowState;
use super::timer::format_duration;
use super::widgets::{
    draw_log_panel, draw_pipeline_tree, draw_progress_footer, draw_summary_panel,
};

pub fn view(model: &mut Model, frame: &mut Frame) {
    match model.mode {
        Mode::Pipeline => view_pipeline(model, frame),
        Mode::Tests => view_tests(model, frame),
        Mode::Report | Mode::Summary => view_summary(model, frame),
    }
}

fn view_pipeline(model: &mut Model, frame: &mut Frame) {
    let items = model.tree.tree_items().unwrap_or_default();
    let (body, footer) = split_main_footer(frame.area(), FOOTER_HEIGHT);
    let (tree_area, log_area) = split_panels(body, TREE_PANEL_RATIO);

    draw_pipeline_tree(frame, tree_area, &items, &mut model.tree_state);
    draw_log_panel(frame, log_area, "Build log", &mut model.logger_state);
    draw_progress_footer(frame, footer, &model.pipeline);
}

fn view_tests(model: &mut Model, frame: &mut Frame) {
    let title = model.test_title.as_deref().unwrap_or("Tests");
    let row_count = model.test_rows.len();
    let selected = selected_test_index(&model.test_rows);

    let (list_area, log_area) = split_panels(frame.area(), TEST_LIST_PANEL_RATIO);

    let items: Vec<ListItem> = model
        .test_rows
        .iter()
        .map(|row| ListItem::new(format_test_row(row)))
        .collect();
    if let Some(index) = selected {
        model.test_list_state.select(Some(index));
    }
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} ({row_count}) ")),
    ).highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_stateful_widget(list, list_area, &mut model.test_list_state);
    draw_log_panel(frame, log_area, "Test log", &mut model.logger_state);
}

fn view_summary(model: &mut Model, frame: &mut Frame) {
    let summary = model.command_summary.clone();
    draw_summary_panel(frame, frame.area(), &summary, &mut model.logger_state);
}

fn selected_test_index(rows: &[super::test_table::TestRow]) -> Option<usize> {
    if rows.is_empty() {
        return None;
    }
    Some(
        rows.iter()
            .position(|row| row.state == TestRowState::Running)
            .or_else(|| rows.iter().rposition(|row| row.state != TestRowState::Pending))
            .unwrap_or(0),
    )
}

fn format_test_row(row: &super::test_table::TestRow) -> Line<'static> {
    let (status, style) = match row.state {
        TestRowState::Pending => ("pending", Style::default().fg(Color::DarkGray)),
        TestRowState::Running => ("running", Style::default().fg(Color::Yellow)),
        TestRowState::Passed => ("pass", Style::default().fg(Color::Green)),
        TestRowState::Failed => ("fail", Style::default().fg(Color::Red)),
        TestRowState::Skipped => ("skip", Style::default().fg(Color::Blue)),
        TestRowState::FilteredOut => ("filt", Style::default().fg(Color::DarkGray)),
    };
    let time = row
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "—".to_owned());
    Line::from(vec![
        Span::styled(format!("{status:<8}"), style),
        Span::raw(format!("{time:>8}  ")),
        Span::raw(row.qualified_name.clone()),
    ])
}
