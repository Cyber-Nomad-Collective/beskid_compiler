//! [`TuiLoggerWidget`](https://deepwiki.com/gin66/tui-logger/4.1-tuiloggerwidget) with build / semantic tabs.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::widgets::{Block, Borders, Tabs, Widget};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetState};

use crate::logging::{activate_tui_log_sink, deactivate_tui_log_sink};

use super::super::log_tabs::{LogTab, LogTabStates};

pub fn init_session_logger() {
    activate_tui_log_sink();
}

pub fn shutdown_session_logger() {
    deactivate_tui_log_sink();
}

/// Tab strip + scrollable log for the active stream.
pub fn draw_tabbed_log_panel(frame: &mut Frame, area: Rect, active: LogTab, log_states: &mut LogTabStates) {
    let [tabs_area, log_area] = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).areas(area);

    let titles: Vec<&str> = LogTab::ALL.iter().map(|tab| tab.title()).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::LEFT | Borders::RIGHT | Borders::TOP))
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .select(active.index())
        .divider(symbols::DOT)
        .padding(" ", " ");
    frame.render_widget(tabs, tabs_area);

    draw_log_panel(frame, log_area, active.scroll_hint(), log_states.state_mut(active));
}

/// Scrollable log view in follow mode (newest lines at the bottom).
pub fn draw_log_panel(frame: &mut Frame, area: Rect, title: &str, logger_state: &mut TuiWidgetState) {
    let widget = TuiLoggerWidget::default()
        .block(Block::default().borders(Borders::ALL).title(format!(" {title} ")))
        .style_info(Style::default().fg(Color::Cyan))
        .style_warn(Style::default().fg(Color::Yellow))
        .style_error(Style::default().fg(Color::Red))
        .style_debug(Style::default().fg(Color::Gray))
        .style_trace(Style::default().fg(Color::DarkGray))
        .output_separator(':')
        .output_level(Some(TuiLoggerLevelOutput::Abbreviated))
        .output_timestamp(None)
        .output_target(true)
        .output_file(false)
        .output_line(false)
        .state(logger_state);
    widget.render(area, frame.buffer_mut());
}
