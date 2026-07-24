//! Summary overlay: chart, failed-test explorer, and code viewer.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::pipeline::tui::format_duration;
use crate::pipeline::tui::widgets::{draw_summary_chart_panel, draw_summary_headline_footer};
use crate::tui::effects::ShellEffect;
use crate::tui::input::{InputEvent, InputResult};
use crate::tui::message::ShellMessage;
use crate::tui::shell::focus::OverlayKind;
use crate::tui::shell::input;
use crate::tui::shell::state::ShellState;

pub fn update(msg: &ShellMessage, state: &mut ShellState) -> Vec<ShellEffect> {
    if matches!(
        msg,
        ShellMessage::SetOverlayVisible { kind: OverlayKind::Summary, visible: true }
            | ShellMessage::ShowTestReport { .. }
            | ShellMessage::StageSummary(_)
    ) {
        state.set_overlay_visible(OverlayKind::Summary, true);
        state.sync_summary_explorer();
    }
    Vec::new()
}

pub fn on_input(event: &InputEvent, state: &mut ShellState) -> InputResult {
    input::handle_summary_overlay_input(event, state)
}

pub fn render(area: Rect, frame: &mut Frame, state: &mut ShellState) {
    let [top, bottom] = Layout::vertical([Constraint::Length(8), Constraint::Min(6)]).areas(area);
    let [chart, headline] = Layout::vertical([Constraint::Min(4), Constraint::Length(3)]).areas(top);
    draw_summary_chart_panel(frame, chart, &state.command_summary);
    draw_summary_headline_footer(frame, headline, &state.command_summary);

    let [explorer, code] = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(bottom);
    let selected_title = draw_failed_explorer(frame, explorer, state);
    state.code_viewer.draw(frame, code, selected_title.as_deref());
}

fn draw_failed_explorer(frame: &mut Frame, area: Rect, state: &mut ShellState) -> Option<String> {
    let failed = state.failed_test_indices();
    if failed.is_empty() {
        return None;
    }
    if state.summary_explorer_index >= failed.len() {
        state.summary_explorer_index = failed.len().saturating_sub(1);
    }
    let items: Vec<ListItem> = failed
        .iter()
        .filter_map(|&row_index| state.test_rows.get(row_index))
        .map(|row| {
            let time = row.duration.map(format_duration).unwrap_or_else(|| "—".to_owned());
            ListItem::new(Line::from(vec![
                Span::styled("fail", Style::default().fg(Color::Red)),
                Span::raw(format!(" {time:>8}  ")),
                Span::raw(row.qualified_name.clone()),
            ]))
        })
        .collect();
    let mut list_state = ListState::default();
    list_state.select(Some(state.summary_explorer_index));
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Failed tests "));
    frame.render_stateful_widget(list, area, &mut list_state);
    failed
        .get(state.summary_explorer_index)
        .and_then(|&row_index| state.test_rows.get(row_index))
        .map(|row| row.qualified_name.clone())
}
