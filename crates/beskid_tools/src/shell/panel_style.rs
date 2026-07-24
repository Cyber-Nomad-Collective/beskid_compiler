//! Flat panel chrome — title lines without nested box borders.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};

/// Accent title line for a panel body (no surrounding border).
pub fn title_line(title: impl Into<String>) -> Line<'static> {
    let title = title.into();
    Line::from(vec![
        Span::styled(title.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled("─".repeat(title.len().max(8)), Style::default().fg(Color::DarkGray)),
    ])
}

/// Minimal block: top rule only (for tabs/toolbars).
pub fn toolbar_block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title)
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
}

/// Dropdown / popover list container.
pub fn popover_block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Indexed(235)))
        .title(title)
        .title_style(Style::default().fg(Color::Cyan))
}
