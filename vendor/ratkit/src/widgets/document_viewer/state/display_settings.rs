//! Display settings shared by document viewers.

/// Presentation toggles for read-only documents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplaySettings {
    /// Whether to show a line-number gutter.
    pub show_line_numbers: bool,
    /// Whether line numbers should be relative to the current line.
    pub relative_line_numbers: bool,
    /// Whether to highlight the current line.
    pub highlight_current_line: bool,
    /// Whether to show an outline pane when entries exist.
    pub show_outline: bool,
    /// Number of spaces used when expanding tabs.
    pub tab_width: usize,
}

impl Default for DisplaySettings {
    /// Creates default display settings.
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            relative_line_numbers: false,
            highlight_current_line: false,
            show_outline: false,
            tab_width: 4,
        }
    }
}

impl DisplaySettings {
    /// Sets absolute or hidden line-number rendering.
    pub fn set_show_line_numbers(&mut self, show: bool) {
        self.show_line_numbers = show;
    }

    /// Sets relative line-number rendering.
    pub fn set_relative_line_numbers(&mut self, relative: bool) {
        self.relative_line_numbers = relative;
    }
}
