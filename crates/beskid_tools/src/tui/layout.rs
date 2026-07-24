//! Terminal layout for the Beskid shell (ratatui constraints + centered overlays).

use ratatui::layout::{Constraint, Layout, Rect};

use crate::shell::chrome::PINNED_TOP_ROWS;
use crate::tui::shell::state::LayoutRects;

pub use crate::shell::layout::overlays::{
    OVERLAY_ANALYSIS, OVERLAY_COMPILE_DEBUG, OVERLAY_GRAPH, OVERLAY_PCKG, OVERLAY_SETTINGS, OVERLAY_SUMMARY,
    OVERLAY_TEMPLATES, OVERLAY_TESTS, overlay_rect, overlay_rect_for,
};

pub const PANEL_HEADER: &str = "header";
pub const PANEL_STAGE: &str = "stage";
pub const PANEL_DETAIL: &str = "detail";
pub const PANEL_LOG: &str = "log";
pub const PANEL_FOOTER: &str = "footer";

pub fn panel_kinds() -> [&'static str; 5] {
    [PANEL_HEADER, PANEL_STAGE, PANEL_DETAIL, PANEL_LOG, PANEL_FOOTER]
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
    let footer = Rect { height: footer_block.height.saturating_sub(1), ..footer_block };
    let [stage, detail] = Layout::horizontal([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)]).areas(body);

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
