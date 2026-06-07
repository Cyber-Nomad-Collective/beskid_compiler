//! [`TuiLoggerWidget`](https://deepwiki.com/gin66/tui-logger/4.1-tuiloggerwidget) build-log panel.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Widget};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetState};

use crate::logging::{activate_tui_log_sink, deactivate_tui_log_sink};

pub fn init_session_logger() {
    activate_tui_log_sink();
}

pub fn shutdown_session_logger() {
    deactivate_tui_log_sink();
}

/// Scrollable log view in follow mode (newest lines at the bottom).
pub fn draw_log_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    logger_state: &mut TuiWidgetState,
) {
    let widget = TuiLoggerWidget::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} ")),
        )
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
