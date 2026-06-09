//! Resolve panes layout to ratatui rects.

use panes::runtime::LayoutRuntime;
use panes_ratatui::TerminalFrame;
use ratatui::layout::Rect;

const CHROME_ROWS: u16 = 1;

pub struct ResolvedPanels<'a> {
    pub frame: TerminalFrame<'a>,
    pub main_area: Rect,
    pub chrome_area: Rect,
}

pub fn resolve_panels<'a>(
    runtime: &'a mut LayoutRuntime,
    area: Rect,
) -> Result<ResolvedPanels<'a>, String> {
    let main_h = area.height.saturating_sub(CHROME_ROWS);
    let main_area = Rect {
        width: area.width,
        height: main_h,
        x: area.x,
        y: area.y,
    };
    let chrome_area = Rect {
        width: area.width,
        height: CHROME_ROWS.min(area.height),
        x: area.x,
        y: area.y + main_h,
    };
    let frame = panes_ratatui::resolve(runtime, main_area)
        .map_err(|e| e.to_string())?;
    Ok(ResolvedPanels {
        frame,
        main_area,
        chrome_area,
    })
}
