//! Centered modal overlay geometry (shared by hi shell and pipeline compile TUI).

use ratatui::layout::Rect;

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
const OVERLAY_GRAPH_W: u16 = 80;
const OVERLAY_GRAPH_H: u16 = 24;
const OVERLAY_SETTINGS_W: u16 = 78;
const OVERLAY_SETTINGS_H: u16 = 22;
const OVERLAY_ANALYSIS_W: u16 = 80;
const OVERLAY_ANALYSIS_H: u16 = 24;

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

pub fn graph_overlay_size() -> (u16, u16) {
    (OVERLAY_GRAPH_W, OVERLAY_GRAPH_H)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_overlay_meets_minimum_size() {
        let (w, h) = graph_overlay_size();
        assert!(w >= 80);
        assert!(h >= 24);
    }

    #[test]
    fn overlay_rect_centers_in_area() {
        let area = Rect::new(0, 0, 100, 30);
        let rect = overlay_rect_for(OVERLAY_GRAPH, area);
        assert_eq!(rect.width, 80);
        assert_eq!(rect.height, 24);
        assert_eq!(rect.x, 10);
        assert_eq!(rect.y, 3);
    }
}
