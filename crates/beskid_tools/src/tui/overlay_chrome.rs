//! Shell Pane + HotkeyFooter chrome for modal overlays.

use crate::shell::primitives::{HotkeyFooter, HotkeyItem};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Clear};
use ratkit::widgets::Pane;

/// Dim the terminal behind an overlay.
pub fn draw_backdrop(frame: &mut Frame, area: Rect) {
    let block = Block::default().style(Style::default().bg(Color::Indexed(234)).fg(Color::DarkGray));
    frame.render_widget(block, area);
}

/// Render a ratkit dialog-style panel: bordered title + body + hotkey footer row.
pub fn render_overlay_panel<F>(frame: &mut Frame, area: Rect, title: &str, hotkeys: &[HotkeyItem], draw_body: F)
where
    F: FnOnce(Rect, &mut Frame),
{
    frame.render_widget(Clear, area);
    let pane = Pane::new(title)
        .with_uniform_padding(0)
        .with_footer_height(1)
        .border_style(Style::default().fg(Color::Cyan))
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    let footer = HotkeyFooter::new(hotkeys.to_vec())
        .key_color(Color::Cyan)
        .description_color(Color::DarkGray)
        .background_color(Color::Indexed(235));
    let (body, foot) = pane.render_block(frame, area);
    draw_body(body, frame);
    if let Some(footer_area) = foot {
        frame.render_widget(&footer, footer_area);
    } else if area.height > 1 {
        let footer_area = Rect { x: area.x, y: area.y + area.height.saturating_sub(1), width: area.width, height: 1 };
        frame.render_widget(&footer, footer_area);
    }
}

pub fn hotkey(key: &str, description: &str) -> HotkeyItem {
    HotkeyItem::new(key, description)
}
