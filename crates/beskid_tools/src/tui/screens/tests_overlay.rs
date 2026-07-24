//! Tests overlay: test list + source/diagnostic code viewer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::pipeline::tui::hyperlink::maybe_link_label;
use crate::pipeline::tui::{TestRow, TestRowState, format_duration};
use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::input;
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
    match msg {
        ShellMessage::SetOverlayVisible { kind: OverlayKind::Tests, visible: true } => {
            state.set_overlay_visible(OverlayKind::Tests, true);
            state.sync_code_viewer_for_selection();
        }
        ShellMessage::BeginTests { .. } | ShellMessage::UpdateTestRows(_) => {
            state.sync_code_viewer_for_selection();
        }
        _ => {}
    }
    Vec::new()
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    input::handle_tests_overlay_input(event, state)
}

pub fn render(area: Rect, frame: &mut Frame, state: &mut ShellState) {
    let [list_area, code_area] =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(area);

    draw_test_list(frame, list_area, state);
    let title =
        state.test_rows.get(state.test_list_state.selected().unwrap_or(0)).map(|row| row.qualified_name.as_str());
    state.code_viewer.draw(frame, code_area, title);
}

fn draw_test_list(frame: &mut Frame, area: Rect, state: &mut ShellState) {
    let row_count = state.test_rows.len();
    if !state.test_list_user_selected
        && let Some(index) = selected_test_index(&state.test_rows)
    {
        state.test_list_state.select(Some(index));
    }
    let items: Vec<ListItem> = state.test_rows.iter().map(|row| ListItem::new(format_test_row(row))).collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(" cases ({row_count}) ")))
        .highlight_style(Style::default().bg(Color::DarkGray));
    frame.render_stateful_widget(list, area, &mut state.test_list_state);
}

fn selected_test_index(rows: &[TestRow]) -> Option<usize> {
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

fn format_test_row(row: &TestRow) -> Line<'static> {
    let (status, style) = match row.state {
        TestRowState::Pending => ("pending", Style::default().fg(Color::DarkGray)),
        TestRowState::Running => ("running", Style::default().fg(Color::Yellow)),
        TestRowState::Passed => ("pass", Style::default().fg(Color::Green)),
        TestRowState::Failed => ("fail", Style::default().fg(Color::Red)),
        TestRowState::Skipped => ("skip", Style::default().fg(Color::Blue)),
        TestRowState::FilteredOut => ("filt", Style::default().fg(Color::DarkGray)),
    };
    let time = row.duration.map(format_duration).unwrap_or_else(|| "—".to_owned());
    Line::from(vec![
        Span::styled(format!("{status:<8}"), style),
        Span::raw(format!("{time:>8}  ")),
        Span::raw(maybe_link_label(row.link.as_ref(), &row.qualified_name, false)),
    ])
}
