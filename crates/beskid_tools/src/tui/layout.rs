//! Terminal layout for the Beskid shell (ratatui constraints + centered overlays).

use ratatui::layout::{Constraint, Layout, Rect};

use crate::shell::chrome::PINNED_TOP_ROWS;
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
pub const OVERLAY_COMPILE_DEBUG: &str = "compile_debug";
pub const OVERLAY_GRAPH: &str = "graph";
pub const OVERLAY_SETTINGS: &str = "settings";
pub const OVERLAY_ANALYSIS: &str = "analysis";

const OVERLAY_TESTS_W: u16 = 72;
const OVERLAY_TESTS_H: u16 = 20;
const OVERLAY_SUMMARY_W: u16 = 72;
const OVERLAY_SUMMARY_H: u16 = 22;
const OVERLAY_PCKG_W: u16 = 78;
const OVERLAY_PCKG_H: u16 = 22;
const OVERLAY_TEMPLATES_W: u16 = 78;
const OVERLAY_TEMPLATES_H: u16 = 22;
const OVERLAY_COMPILE_DEBUG_W: u16 = 80;
const OVERLAY_COMPILE_DEBUG_H: u16 = 24;
const OVERLAY_GRAPH_W: u16 = 72;
const OVERLAY_GRAPH_H: u16 = 20;
const OVERLAY_SETTINGS_W: u16 = 78;
const OVERLAY_SETTINGS_H: u16 = 22;
const OVERLAY_ANALYSIS_W: u16 = 72;
const OVERLAY_ANALYSIS_H: u16 = 20;

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
    let [header, body, log, footer_block] = Layout::vertical([
        Constraint::Length(PINNED_TOP_ROWS),
        Constraint::Min(0),
        Constraint::Length(8),
        Constraint::Length(6),
    ])
    .areas(area);
    let footer = Rect {
        height: footer_block.height.saturating_sub(1),
        ..footer_block
    };
    let [stage, detail] = Layout::horizontal([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
        .areas(body);

    let chrome = Rect {
        y: footer_block.y + footer.height,
        height: 1.min(footer_block.height),
        x: footer_block.x,
        width: footer_block.width,
    };
    LayoutRects {
        header,
        stage,
        detail,
        log,
        footer,
        chrome,
        tests_overlay: None,
        summary_overlay: None,
        pckg_overlay: None,
        templates_overlay: None,
        compile_debug_overlay: None,
        graph_overlay: None,
        settings_overlay: None,
        analysis_overlay: None,
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
        OVERLAY_COMPILE_DEBUG => {
            overlay_rect(area, OVERLAY_COMPILE_DEBUG_W, OVERLAY_COMPILE_DEBUG_H)
        }
        OVERLAY_GRAPH => overlay_rect(area, OVERLAY_GRAPH_W, OVERLAY_GRAPH_H),
        OVERLAY_SETTINGS => overlay_rect(area, OVERLAY_SETTINGS_W, OVERLAY_SETTINGS_H),
        OVERLAY_ANALYSIS => overlay_rect(area, OVERLAY_ANALYSIS_W, OVERLAY_ANALYSIS_H),
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
        assert_eq!(rects.header.height, PINNED_TOP_ROWS);
        assert_eq!(rects.log.height, 8);
        assert_eq!(rects.footer.height, 5);
        assert!(rects.stage.width + rects.detail.width <= area.width);
    }
}
