//! Permanent shell chrome: header scope bar + footer hotkeys.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratkit::widgets::{HotkeyFooter, HotkeyItem};

use super::hotkeys::ShellHotkeys;
use super::scope::ShellScope;

pub struct ShellChrome {
    pub show_help: bool,
}

impl Default for ShellChrome {
    fn default() -> Self {
        Self { show_help: false }
    }
}

impl ShellChrome {
    pub fn render_header(&self, area: Rect, frame: &mut Frame, scope: &ShellScope, title: &str) {
        let scope_label = match scope {
            ShellScope::User => "user",
            ShellScope::Project { .. } => "project",
            ShellScope::Workspace { .. } => "workspace",
        };
        let line = Line::from(vec![
            Span::styled("Beskid", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" · "),
            Span::styled(title, Style::default().fg(Color::Yellow)),
            Span::raw(" · "),
            Span::styled(scope_label, Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(
            Paragraph::new(line).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Beskid Hi "),
            ),
            area,
        );
    }

    pub fn render_footer(
        &self,
        area: Rect,
        frame: &mut Frame,
        hotkeys: &ShellHotkeys,
        focused_widget: Option<&str>,
    ) {
        let items = hotkeys.footer_items(focused_widget);
        let footer = HotkeyFooter::new(items)
            .key_color(Color::Cyan)
            .description_color(Color::DarkGray)
            .background_color(Color::Indexed(235));
        frame.render_widget(footer, area);
    }

    pub fn render_help_overlay(&self, area: Rect, frame: &mut Frame, items: &[HotkeyItem]) {
        let lines: Vec<Line> = items
            .iter()
            .map(|item| {
                Line::from(vec![
                    Span::styled(
                        format!("{}  ", item.key),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(item.description.clone(), Style::default().fg(Color::Gray)),
                ])
            })
            .collect();
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Shortcuts ")
            .style(Style::default().bg(Color::Indexed(234)));
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }
}
