//! Permanent shell chrome: pinned top bar + footer hotkeys.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use super::primitives::{HotkeyFooter, HotkeyItem};

pub type FooterHotkeyItem = HotkeyItem;

use super::control_mode::HiControlMode;
use super::hotkeys::ShellHotkeys;
use super::phase::transition_label;
use super::scope::ShellScope;
use super::top_menu::ShellTopMenu;
use crate::tui::shell::state::ShellState;

/// Fixed height of the pinned top bar (not layout-editable).
pub const PINNED_TOP_ROWS: u16 = 2;

pub struct ShellChrome {
    pub show_help: bool,
}

impl Default for ShellChrome {
    fn default() -> Self {
        Self { show_help: false }
    }
}

impl ShellChrome {
    /// Pinned top bar: Beskid branding, workflow phase, scope, and horizontal menu.
    pub fn render_pinned_top_bar(
        &self,
        area: Rect,
        frame: &mut Frame,
        scope: &ShellScope,
        page_title: &str,
        shell_state: &ShellState,
        menu: &mut ShellTopMenu,
    ) {
        if area.height < 2 {
            return;
        }
        let [brand_row, menu_row] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        let scope_label = scope.chrome_title();
        let phase = transition_label(shell_state, page_title);
        let brand_line = Line::from(vec![
            Span::styled(
                "Beskid",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(phase, Style::default().fg(Color::Yellow)),
            Span::raw("   "),
            Span::styled(scope_label, Style::default().fg(Color::DarkGray)),
            Span::raw("   "),
            Span::styled("F10", Style::default().fg(Color::Cyan)),
            Span::styled(" menu", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(brand_line), brand_row);
        menu.render_menu_row(menu_row, frame);
    }

    pub fn render_footer(
        &self,
        area: Rect,
        frame: &mut Frame,
        hotkeys: &ShellHotkeys,
        mode: HiControlMode,
        focused_widget: Option<&str>,
        layout_drawer_visible: bool,
    ) {
        let items = hotkeys.footer_for_mode(mode, focused_widget, layout_drawer_visible);
        let footer = HotkeyFooter::new(items)
            .key_color(Color::Cyan)
            .description_color(Color::DarkGray)
            .background_color(Color::Indexed(235));
        frame.render_widget(footer, area);
    }

    pub fn render_help_overlay(&self, area: Rect, frame: &mut Frame, items: &[HotkeyItem]) {
        use ratatui::widgets::{Block, Borders};
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
