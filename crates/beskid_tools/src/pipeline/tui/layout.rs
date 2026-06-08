//! Ratatui layout recipes (nested splits, footer, summary geometry).

use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};

pub const FOOTER_HEIGHT: u16 = 5;
pub const TREE_PANEL_RATIO: u16 = 42;
pub const TEST_LIST_PANEL_RATIO: u16 = 45;
pub const SUMMARY_HEADLINE_HEIGHT: u16 = 3;

/// Vertical split: scrollable body + fixed footer.
pub fn split_main_footer(area: Rect, footer_h: u16) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(footer_h)])
        .flex(Flex::Legacy)
        .split(area);
    (chunks[0], chunks[1])
}

/// Horizontal split for side-by-side panels.
pub fn split_panels(area: Rect, left_pct: u16) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(100 - left_pct),
        ])
        .flex(Flex::Legacy)
        .split(area);
    (chunks[0], chunks[1])
}

/// Summary screen: body + headline footer.
pub fn split_summary_root(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),
            Constraint::Length(SUMMARY_HEADLINE_HEIGHT),
        ])
        .flex(Flex::Legacy)
        .split(area);
    (chunks[0], chunks[1])
}

/// Summary body: chart/table left, log right.
pub fn split_summary_body(area: Rect) -> (Rect, Rect) {
    split_panels(area, 40)
}

/// Progress footer: stage gauge over total gauge.
pub fn split_progress_footer(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .flex(Flex::SpaceBetween)
        .split(area);
    (chunks[0], chunks[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn splits_produce_non_zero_areas_on_standard_terminal() {
        let area = Rect::new(0, 0, 80, 24);
        let (body, footer) = split_main_footer(area, FOOTER_HEIGHT);
        assert!(body.height > 0);
        assert_eq!(footer.height, FOOTER_HEIGHT);

        let (left, right) = split_panels(body, TREE_PANEL_RATIO);
        assert!(left.width > 0);
        assert!(right.width > 0);
    }
}
