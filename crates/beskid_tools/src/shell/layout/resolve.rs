//! Resolve panes layout to ratatui rects.

use panes::PanelId;
use panes::runtime::LayoutRuntime;
use panes_ratatui::TerminalFrame;
use ratatui::layout::Rect;

use crate::shell::chrome::PINNED_TOP_ROWS;

const CHROME_ROWS: u16 = 1;

pub struct ResolvedPanels<'a> {
    pub frame: TerminalFrame<'a>,
    pub header_area: Rect,
    pub main_area: Rect,
    pub chrome_area: Rect,
}

pub fn resolve_panels<'a>(
    runtime: &'a mut LayoutRuntime,
    area: Rect,
) -> Result<ResolvedPanels<'a>, String> {
    let header_h = PINNED_TOP_ROWS.min(area.height);
    let chrome_h = CHROME_ROWS.min(area.height.saturating_sub(header_h));
    let main_h = area.height.saturating_sub(header_h).saturating_sub(chrome_h);
    let header_area = Rect {
        width: area.width,
        height: header_h,
        x: area.x,
        y: area.y,
    };
    let main_area = Rect {
        width: area.width,
        height: main_h,
        x: area.x,
        y: area.y + header_h,
    };
    let chrome_area = Rect {
        width: area.width,
        height: chrome_h,
        x: area.x,
        y: area.y + header_h + main_h,
    };
    let frame = panes_ratatui::resolve(runtime, main_area)
        .map_err(|e| e.to_string())?;
    Ok(ResolvedPanels {
        frame,
        header_area,
        main_area,
        chrome_area,
    })
}

/// Panel under a terminal cell, if any (coordinates relative to full frame).
pub fn panel_id_at_terminal(
    frame: &TerminalFrame<'_>,
    main_area: Rect,
    column: u16,
    row: u16,
) -> Option<PanelId> {
    if column < main_area.x
        || row < main_area.y
        || column >= main_area.x + main_area.width
        || row >= main_area.y + main_area.height
    {
        return None;
    }
    let inner = frame.inner()?;
    let layout = inner.layout();
    let x = f32::from(column - main_area.x);
    let y = f32::from(row - main_area.y);
    layout.panel_at_point(x, y)
}

/// Focus the panel under a terminal cell, if any (coordinates relative to full frame).
pub fn focus_panel_at_terminal(
    runtime: &mut LayoutRuntime,
    frame: &TerminalFrame<'_>,
    main_area: Rect,
    column: u16,
    row: u16,
) -> bool {
    let Some(pid) = panel_id_at_terminal(frame, main_area, column, row) else {
        return false;
    };
    runtime.focus(pid);
    true
}

/// Focus the first live panel matching a widget kind string.
pub fn focus_panel_by_kind(runtime: &mut LayoutRuntime, kind: &str) -> bool {
    let Some(&pid) = runtime.tree().panels_by_kind(kind).first() else {
        return false;
    };
    runtime.focus(pid);
    true
}

pub fn panel_id_for_kind(runtime: &LayoutRuntime, kind: &str) -> Option<PanelId> {
    runtime.tree().panels_by_kind(kind).first().copied()
}
