//! TEA view: unified flexible shell with stage-aware pane content.

use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::layout::{split_main_panes, split_shell};
use super::model::{Mode, Model};
use super::stage_focus::StageFocus;
use super::test_table::TestRowState;
use super::timer::format_duration;
use super::widgets::{
    draw_context_bar, draw_log_panel, draw_pipeline_tree, draw_progress_footer,
    draw_stage_panel, draw_summary_chart_panel, draw_summary_headline_footer,
};

pub fn view(model: &mut Model, frame: &mut Frame) {
    let focus = StageFocus::from_model(model);
    let areas = split_shell(frame.area(), focus);
    let (primary, secondary) = split_main_panes(areas.main, focus);

    draw_context_bar(frame, areas.header, model, focus);
    draw_stage_panel(frame, primary, model, focus);
    draw_secondary_pane(frame, secondary, model);
    draw_log_panel(frame, areas.log, log_title(model.mode), &mut model.logger_state);

    match model.mode {
        Mode::Report | Mode::Summary => {
            draw_summary_headline_footer(frame, areas.footer, &model.command_summary);
        }
        _ => draw_progress_footer(frame, areas.footer, &model.pipeline),
    }
}

fn log_title(mode: Mode) -> &'static str {
    match mode {
        Mode::Tests => "Test log",
        Mode::Report | Mode::Summary => "Log",
        Mode::Pipeline => "Build log",
    }
}

fn draw_secondary_pane(frame: &mut Frame, area: ratatui::layout::Rect, model: &mut Model) {
    match model.mode {
        Mode::Pipeline => {
            let items = model.tree.tree_items().unwrap_or_default();
            let title = StageFocus::from_model(model).title();
            draw_pipeline_tree(frame, area, &items, &mut model.tree_state, title);
        }
        Mode::Tests => draw_test_list(frame, area, model),
        Mode::Report | Mode::Summary => {
            draw_summary_chart_panel(frame, area, &model.command_summary);
        }
    }
}

fn draw_test_list(frame: &mut Frame, area: ratatui::layout::Rect, model: &mut Model) {
    let title = model.test_title.as_deref().unwrap_or("Tests");
    let row_count = model.test_rows.len();
    if let Some(index) = selected_test_index(&model.test_rows) {
        model.test_list_state.select(Some(index));
    }
    let items: Vec<ListItem> = model
        .test_rows
        .iter()
        .map(|row| ListItem::new(format_test_row(row)))
        .collect();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {title} ({row_count}) ")),
    ).highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_stateful_widget(list, area, &mut model.test_list_state);
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
