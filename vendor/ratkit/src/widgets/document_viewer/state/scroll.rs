//! Scroll and viewport state shared by document viewers.

use ratatui::layout::Rect;

/// Tracks focused line and viewport offset for read-only documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollState {
    /// One-based current focused line.
    pub current_line: usize,
    /// Zero-based first visible line for shared document rendering.
    pub offset: usize,
    /// Zero-based first visible line for markdown-compatible callers.
    pub scroll_offset: usize,
    /// Height of the visible viewport.
    pub viewport_height: usize,
    /// Total renderable lines.
    pub total_lines: usize,
    /// Current filter text for filter-mode viewers.
    pub filter: Option<String>,
    /// Whether filter mode is active.
    pub filter_mode: bool,
}

impl ScrollState {
    /// Creates a new scroll state.
    pub fn new() -> Self {
        Self {
            current_line: 1,
            offset: 0,
            scroll_offset: 0,
            viewport_height: 20,
            total_lines: 0,
            filter: None,
            filter_mode: false,
        }
    }
    /// Updates total lines and clamps current viewport state.
    pub fn update_total_lines(&mut self, total_lines: usize) {
        self.total_lines = total_lines.max(1);
        self.current_line = self.current_line.clamp(1, self.total_lines);
        self.clamp_offsets();
    }
    /// Moves focus to an absolute zero-based line index.
    pub fn scroll_to(&mut self, line: usize) {
        self.set_current_line(line.saturating_add(1));
    }
    /// Sets the current one-based line and keeps it visible.
    pub fn set_current_line(&mut self, line: usize) {
        self.current_line = line.clamp(1, self.total_lines.max(1));
        self.adjust_scroll_for_current_line();
    }
    /// Scrolls the viewport down by a number of lines.
    pub fn scroll_down(&mut self, amount: usize) {
        self.set_offset(self.effective_offset().saturating_add(amount));
    }
    /// Scrolls the viewport up by a number of lines.
    pub fn scroll_up(&mut self, amount: usize) {
        self.set_offset(self.effective_offset().saturating_sub(amount));
    }
    /// Moves the focused line down by a number of lines.
    pub fn move_current_down(&mut self, amount: usize) {
        self.set_current_line(self.current_line.saturating_add(amount));
    }
    /// Moves the focused line up by a number of lines.
    pub fn move_current_up(&mut self, amount: usize) {
        self.set_current_line(self.current_line.saturating_sub(amount).max(1));
    }
    /// Moves current line down by one.
    pub fn line_down(&mut self) {
        self.move_current_down(1);
    }
    /// Moves current line up by one.
    pub fn line_up(&mut self) {
        self.move_current_up(1);
    }
    /// Moves to the first line.
    pub fn scroll_to_top(&mut self) {
        self.current_line = 1;
        self.set_offset(0);
    }
    /// Moves to the last line.
    pub fn scroll_to_bottom(&mut self) {
        self.current_line = self.total_lines.max(1);
        self.set_offset(self.max_scroll_offset());
    }
    /// Moves to the next filter match placeholder.
    pub fn filter_line_down(&mut self, _filter_text: String) {
        self.line_down();
    }
    /// Moves to the previous filter match placeholder.
    pub fn filter_line_up(&mut self, _filter_text: String) {
        self.line_up();
    }
    /// Updates viewport dimensions from a render area.
    pub fn update_viewport(&mut self, area: Rect) {
        self.viewport_height = area.height as usize;
        self.adjust_scroll_for_current_line();
    }
    /// Keeps the current line visible within a viewport height.
    pub fn ensure_current_visible(&mut self, height: usize) {
        self.viewport_height = height.max(1);
        self.adjust_scroll_for_current_line();
    }
    /// Returns the zero-based visible range for shared rendering.
    pub fn visible_range(&self, height: usize) -> std::ops::Range<usize> {
        let start = self.effective_offset();
        let end = start.saturating_add(height).min(self.total_lines.max(1));
        start..end
    }
    /// Returns true when the current line is inside the viewport.
    pub fn is_current_line_visible(&self) -> bool {
        let first_visible = self.effective_offset().saturating_add(1);
        let last_visible = self
            .effective_offset()
            .saturating_add(self.viewport_height.max(1));
        self.current_line >= first_visible && self.current_line <= last_visible
    }
    /// Returns the maximum valid zero-based viewport offset.
    pub fn max_scroll_offset(&self) -> usize {
        self.total_lines.saturating_sub(self.viewport_height.max(1))
    }
    /// Calculates scroll percentage from 0.0 to 1.0.
    pub fn scroll_percentage(&self) -> f64 {
        let max_offset = self.max_scroll_offset();
        if max_offset == 0 {
            0.0
        } else {
            self.effective_offset() as f64 / max_offset as f64
        }
    }
    /// Returns the zero-based index for the focused line.
    pub fn current_line_index(&self) -> usize {
        self.current_line.saturating_sub(1)
    }
    /// Returns the zero-based effective viewport offset.
    pub fn effective_offset(&self) -> usize {
        self.scroll_offset
            .max(self.offset)
            .min(self.max_scroll_offset())
    }
    /// Sets both public offset aliases.
    pub fn set_offset(&mut self, offset: usize) {
        let offset = offset.min(self.max_scroll_offset());
        self.offset = offset;
        self.scroll_offset = offset;
    }
    /// Keeps current line visible using markdown's margin behavior.
    pub fn adjust_scroll_for_current_line(&mut self) {
        const SCROLL_MARGIN: usize = 3;
        let viewport_height = self.viewport_height.max(1);
        let current = self.current_line_index();
        let mut offset = self.effective_offset();
        if viewport_height <= SCROLL_MARGIN * 2 {
            if current < offset {
                offset = current;
            } else if current >= offset.saturating_add(viewport_height) {
                offset = current.saturating_sub(viewport_height.saturating_sub(1));
            }
            self.set_offset(offset);
            return;
        }
        let first_visible = offset;
        let last_visible = offset.saturating_add(viewport_height.saturating_sub(1));
        if current < first_visible.saturating_add(SCROLL_MARGIN) {
            offset = current.saturating_sub(SCROLL_MARGIN);
        } else if current > last_visible.saturating_sub(SCROLL_MARGIN) {
            offset = current
                .saturating_add(SCROLL_MARGIN)
                .saturating_sub(viewport_height.saturating_sub(1));
        }
        self.set_offset(offset);
    }
    /// Clamps both offset aliases to the valid range.
    fn clamp_offsets(&mut self) {
        let offset = self.effective_offset();
        self.set_offset(offset);
    }
}
impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}
