//! Ratatui layout recipes (nested splits, footer, summary geometry).

use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};

use super::stage_focus::StageFocus;

pub const FOOTER_HEIGHT: u16 = 5;
pub const CONTEXT_BAR_HEIGHT: u16 = 3;
pub const SUMMARY_HEADLINE_HEIGHT: u16 = 3;

/// Rectangles for the unified pipeline shell (header → main → log → footer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellAreas {
    pub header: Rect,
    pub main: Rect,
    pub log: Rect,
    pub footer: Rect,
}

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
    let left = left_pct.min(100);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left),
            Constraint::Percentage(100 - left),
        ])
        .flex(Flex::Legacy)
        .split(area);
    (chunks[0], chunks[1])
}

/// Unified shell: context bar, flexible main, log strip, progress footer.
pub fn split_shell(area: Rect, focus: StageFocus) -> ShellAreas {
    let log_min = focus.log_min_rows();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(CONTEXT_BAR_HEIGHT),
            Constraint::Min(6),
            Constraint::Min(log_min),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .flex(Flex::Legacy)
        .split(area);
    ShellAreas {
        header: chunks[0],
        main: chunks[1],
        log: chunks[2],
        footer: chunks[3],
    }
}

/// Main body: primary (stage) + secondary (tree / list / chart) with focus-driven ratio.
pub fn split_main_panes(area: Rect, focus: StageFocus) -> (Rect, Rect) {
    split_panels(area, focus.main_split_left_pct())
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
        let shell = split_shell(area, StageFocus::Semantic);
        assert!(shell.main.height > 0);
        assert_eq!(shell.footer.height, FOOTER_HEIGHT);
        assert_eq!(shell.header.height, CONTEXT_BAR_HEIGHT);

        let (left, right) = split_main_panes(shell.main, StageFocus::Semantic);
        assert!(left.width > 0);
        assert!(right.width > 0);
        assert!(left.width < right.width);
    }

    #[test]
    fn workspace_focus_gives_primary_more_width() {
        let area = Rect::new(0, 0, 100, 30);
        let shell = split_shell(area, StageFocus::Workspace);
        let (left, right) = split_main_panes(shell.main, StageFocus::Workspace);
        assert!(left.width > right.width / 2);
    }
}
