//! Terminal layout for the Beskid shell (ratatui constraints + centered overlays).

use ratatui::layout::{Constraint, Layout, Rect};

use crate::tui::shell::state::LayoutRects;

pub const PANEL_HEADER: &str = "header";
pub const PANEL_STAGE: &str = "stage";
pub const PANEL_DETAIL: &str = "detail";
pub const PANEL_LOG: &str = "log";
pub const PANEL_FOOTER: &str = "footer";

pub const OVERLAY_TESTS: &str = "tests";
pub const OVERLAY_SUMMARY: &str = "summary";
pub const OVERLAY_PCKG: &str = "pckg";
pub const OVERLAY_TEMPLATES: &str = "templates";

const OVERLAY_TESTS_W: u16 = 72;
const OVERLAY_TESTS_H: u16 = 20;
const OVERLAY_SUMMARY_W: u16 = 72;
const OVERLAY_SUMMARY_H: u16 = 22;
const OVERLAY_PCKG_W: u16 = 78;
const OVERLAY_PCKG_H: u16 = 22;
const OVERLAY_TEMPLATES_W: u16 = 78;
const OVERLAY_TEMPLATES_H: u16 = 22;

pub fn panel_kinds() -> [&'static str; 5] {
    [
        PANEL_HEADER,
        PANEL_STAGE,
        PANEL_DETAIL,
        PANEL_LOG,
        PANEL_FOOTER,
    ]
}

/// Base panels: header → stage|detail → log → footer.
pub fn resolve_shell_layout(area: Rect) -> LayoutRects {
    let [header, body, log, footer] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(8),
        Constraint::Length(5),
    ])
    .areas(area);
    let [stage, detail] = Layout::horizontal([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
        .areas(body);

    LayoutRects {
        header,
        stage,
        detail,
        log,
        footer,
        tests_overlay: None,
        summary_overlay: None,
        pckg_overlay: None,
        templates_overlay: None,
    }
}

pub fn overlay_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

pub fn overlay_rect_for(kind: &str, area: Rect) -> Rect {
    match kind {
        OVERLAY_TESTS => overlay_rect(area, OVERLAY_TESTS_W, OVERLAY_TESTS_H),
        OVERLAY_SUMMARY => overlay_rect(area, OVERLAY_SUMMARY_W, OVERLAY_SUMMARY_H),
        OVERLAY_PCKG => overlay_rect(area, OVERLAY_PCKG_W, OVERLAY_PCKG_H),
        OVERLAY_TEMPLATES => overlay_rect(area, OVERLAY_TEMPLATES_W, OVERLAY_TEMPLATES_H),
        _ => overlay_rect(area, OVERLAY_SUMMARY_W, OVERLAY_SUMMARY_H),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_layout_resolves_all_panels() {
        let area = Rect::new(0, 0, 80, 24);
        let rects = resolve_shell_layout(area);
        assert_eq!(rects.header.height, 4);
        assert_eq!(rects.log.height, 8);
        assert_eq!(rects.footer.height, 5);
        assert!(rects.stage.width + rects.detail.width <= area.width);
    }
}
