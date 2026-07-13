//! Permanent shell chrome: pinned top bar + footer hotkeys.

use super::primitives::{HotkeyFooter, HotkeyItem};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub type FooterHotkeyItem = HotkeyItem;

use super::control_mode::HiControlMode;
use super::hotkeys::ShellHotkeys;
use super::scope::ShellScope;
use super::shortcut_clicks::{
    ShortcutClickTargets, register_footer_clicks, register_help_overlay_clicks,
};

/// Fixed height of the pinned top bar (not layout-editable).
pub const PINNED_TOP_ROWS: u16 = 1;

#[derive(Default)]
pub struct ShellChrome {
    pub show_help: bool,
}

impl ShellChrome {
    /// Pinned top bar: welcome label and opened workspace/project scope.
    pub fn render_pinned_top_bar(&self, area: Rect, frame: &mut Frame, scope: &ShellScope) {
        if area.height == 0 {
            return;
        }
        let scope_label = scope.chrome_title();
        let line = Line::from(vec![
            Span::styled(
                "Welcome",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled(scope_label, Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_footer(
        &self,
        area: Rect,
        frame: &mut Frame,
        hotkeys: &ShellHotkeys,
        mode: HiControlMode,
        focused_widget: Option<&str>,
        layout_drawer_visible: bool,
        click_targets: &mut ShortcutClickTargets,
    ) {
        let items = hotkeys.footer_for_mode(mode, focused_widget, layout_drawer_visible);
        register_footer_clicks(click_targets, area, &items);
        let footer = HotkeyFooter::new(items)
            .key_color(Color::Cyan)
            .description_color(Color::DarkGray)
            .background_color(Color::Indexed(235));
        frame.render_widget(footer, area);
    }

    pub fn render_help_overlay(
        &self,
        area: Rect,
        frame: &mut Frame,
        items: &[HotkeyItem],
        click_targets: &mut ShortcutClickTargets,
    ) {
        use ratatui::widgets::{Block, Borders};
        register_help_overlay_clicks(click_targets, area, items);
        let lines: Vec<Line> = items
            .iter()
            .map(|item| {
                Line::from(vec![
                    Span::styled(
                        format!("{}  ", item.key),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::shell::layout::load::load_from_source;
    use crate::shell::layout::{EMBEDDED_HI_V2, resolve::resolve_panels};

    #[test]
    fn pinned_chrome_single_row() {
        assert_eq!(PINNED_TOP_ROWS, 1);
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let (_doc, mut runtime) = load_from_source(EMBEDDED_HI_V2).expect("layout");
        let resolved = resolve_panels(&mut runtime, area).expect("resolve");
        assert_eq!(resolved.header_area.height, 1);
    }

    #[test]
    fn chrome_renders_welcome_and_scope() {
        let backend = TestBackend::new(80, 10);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let scope = ShellScope::Workspace {
            root: "/tmp/ws".into(),
            manifest: "/tmp/ws/CoreLib.bws".into(),
        };
        terminal
            .draw(|frame| {
                ShellChrome::default().render_pinned_top_bar(frame.area(), frame, &scope);
            })
            .expect("draw");
        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("Welcome"));
        assert!(text.contains("CoreLib"));
        assert!(!text.contains("Compiling"));
        assert!(!text.contains("Boards"));
    }
}
