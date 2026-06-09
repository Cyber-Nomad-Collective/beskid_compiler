//! Outline rendering and hit-testing for document viewers.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

const PANEL_BG: Color = Color::Rgb(30, 32, 38);
const BORDER_FG: Color = Color::Rgb(138, 99, 210);
const TEXT_FG: Color = Color::Rgb(160, 160, 160);
const HOVER_BG: Color = Color::Rgb(60, 60, 70);
const COMPACT_LINE_FG: Color = Color::Rgb(120, 120, 130);

use crate::widgets::document_viewer::foundation::DocumentOutlineItem;

/// Renders document outline entries as a TOC overlay.
pub fn render_outline(
    area: Rect,
    buf: &mut Buffer,
    outline: &[DocumentOutlineItem],
    hovered: bool,
    hovered_entry: Option<usize>,
) {
    let Some(toc_area) = outline_overlay_area(area, outline.len(), hovered) else {
        return;
    };
    fill_background(toc_area, buf, Style::default().bg(PANEL_BG));
    render_outline_block(toc_area, buf);
    let inner = outline_inner_area(toc_area);
    if hovered {
        render_expanded_entries(inner, buf, outline, hovered_entry);
    } else {
        render_compact_marker(inner, buf);
    }
}

/// Returns the outline entry index under a terminal coordinate.
pub fn outline_entry_at_position(
    x: u16,
    y: u16,
    area: Rect,
    outline_len: usize,
    hovered: bool,
) -> Option<usize> {
    let toc_area = outline_overlay_area(area, outline_len, hovered)?;
    let inner = outline_inner_area(toc_area);
    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }
    let index = y.saturating_sub(inner.y) as usize;
    (index < outline_len).then_some(index)
}

/// Calculates the right-side TOC overlay area.
pub fn outline_overlay_area(area: Rect, item_count: usize, hovered: bool) -> Option<Rect> {
    if area.width < 24 || area.height < 4 || item_count == 0 {
        return None;
    }
    let width = if hovered {
        area.width.clamp(20, 30).min(area.width.saturating_sub(4))
    } else {
        12.min(area.width.saturating_sub(4))
    };
    let wanted_height = if hovered { item_count as u16 + 2 } else { 4 };
    let height = wanted_height.clamp(4, area.height.saturating_sub(1));
    Some(Rect {
        x: area.x + area.width.saturating_sub(width + 2),
        y: area.y + 1,
        width,
        height,
    })
}

/// Returns the content area inside a TOC block.
fn outline_inner_area(area: Rect) -> Rect {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .inner(area)
}

/// Renders the outline block chrome.
fn render_outline_block(area: Rect, buf: &mut Buffer) {
    let panel_style = Style::default().bg(PANEL_BG);
    Block::default()
        .title(" TOC ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FG).bg(PANEL_BG))
        .style(panel_style)
        .render(area, buf);
}

/// Fills a rectangle with a style before overlay chrome is drawn.
fn fill_background(area: Rect, buf: &mut Buffer, style: Style) {
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ').set_style(style);
            }
        }
    }
}

/// Renders expanded outline entries.
fn render_expanded_entries(
    area: Rect,
    buf: &mut Buffer,
    outline: &[DocumentOutlineItem],
    hovered_entry: Option<usize>,
) {
    for (row, item) in outline.iter().take(area.height as usize).enumerate() {
        let prefix = "  ".repeat(item.level.min(3));
        let text = format!("{prefix}{}", item.title);
        let style = if hovered_entry == Some(row) {
            Style::default().fg(Color::White).bg(HOVER_BG)
        } else {
            Style::default().fg(TEXT_FG).bg(PANEL_BG)
        };
        buf.set_stringn(
            area.x,
            area.y + row as u16,
            text,
            area.width as usize,
            style,
        );
    }
}

/// Renders the compact TOC marker.
fn render_compact_marker(area: Rect, buf: &mut Buffer) {
    if area.is_empty() {
        return;
    }
    buf.set_stringn(
        area.x,
        area.y,
        "⠉⢙⣛⣛⣛⣛⣛⣛⣛⣛",
        area.width as usize,
        Style::default().fg(COMPACT_LINE_FG).bg(PANEL_BG),
    );
}
