//! Header: mode tabs, spinner, and compact status (no duplicate gauges).

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};

use super::super::stage_focus::StageFocus;
use super::spinner::draw_status_spinner;
use crate::tui::shell::focus::{FocusTarget, OverlayKind};
use crate::tui::shell::state::ShellState;

fn mode_tab_titles() -> Vec<&'static str> {
    vec!["Pipeline", "Tests", "Summary"]
}

fn active_tab_index(state: &ShellState) -> usize {
    match state.focus {
        FocusTarget::Overlay(OverlayKind::Tests) => 1,
        FocusTarget::Overlay(OverlayKind::Summary) => 2,
        _ => 0,
    }
}

pub fn draw_context_bar(frame: &mut Frame, area: Rect, state: &ShellState, focus: StageFocus) {
    let [top_row, status_row] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);

    let titles = mode_tab_titles();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::TOP | Borders::LEFT | Borders::RIGHT))
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
        .select(active_tab_index(state))
        .divider(symbols::DOT)
        .padding("·", "·");
    frame.render_widget(tabs, top_row);

    let text_area = if state.show_spinner() {
        let [spinner_area, text_area] =
            Layout::horizontal([Constraint::Length(2), Constraint::Min(0)]).areas(status_row);
        draw_status_spinner(frame, spinner_area, state.tick);
        text_area
    } else {
        status_row
    };

    let stage = if state.compile_complete && state.focus.is_base() {
        "complete"
    } else if state.pipeline.stage_label.is_empty() {
        "starting"
    } else {
        state.pipeline.stage_label.as_str()
    };

    let mut spans = vec![
        Span::styled(
            "Beskid",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · "),
        Span::styled(focus.title(), Style::default().fg(Color::Yellow)),
        Span::raw(" · "),
        Span::styled(stage, Style::default().fg(Color::White)),
    ];

    if let Some(work) = state.last_work_unit.as_deref() {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(work, Style::default().fg(Color::DarkGray)));
    }

    spans.push(Span::raw(" · "));
    spans.push(Span::styled(
        format!("focus:{}", state.pane_focus.label()),
        Style::default().fg(Color::DarkGray),
    ));

    if let Some(hint) = state.navigation_hint() {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(hint, Style::default().fg(Color::Green)));
    } else if state.shell_mode == crate::tui::shell::pane_state::ShellMode::ProjectWizard {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(
            "[4] packages · [5] templates · i install · q quit",
            Style::default().fg(Color::Green),
        ));
    } else {
        spans.push(Span::raw(" · "));
        spans.push(Span::styled(
            "[4] pckg · [5] new project",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let widget = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .title(" "),
    );
    frame.render_widget(widget, text_area);
}
