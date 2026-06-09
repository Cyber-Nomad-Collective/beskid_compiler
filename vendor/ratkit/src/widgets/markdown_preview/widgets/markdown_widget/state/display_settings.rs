//! Display settings for markdown widget.
//!
//! Markdown-specific options live here while generic document-viewer display
//! state is delegated to `document_viewer::DisplaySettings`.

use crate::widgets::{
    document_viewer,
    markdown_preview::widgets::markdown_widget::foundation::elements::CodeBlockTheme,
};

/// Display settings for markdown rendering.
#[derive(Debug, Clone)]
pub struct DisplaySettings {
    /// Generic document-viewer presentation settings.
    pub viewer: document_viewer::DisplaySettings,
    /// Whether to show line numbers inside fenced code blocks.
    pub show_line_numbers: bool,
    /// Color theme for markdown code blocks.
    pub code_block_theme: CodeBlockTheme,
    /// Whether to show collapse indicators on headings.
    pub show_heading_collapse: bool,
    /// Scroll multiplier for wheel-based scrolling.
    pub scroll_multiplier: usize,
}

impl DisplaySettings {
    /// Creates new markdown display settings with defaults.
    pub fn new() -> Self {
        Self {
            viewer: document_viewer::DisplaySettings {
                show_line_numbers: false,
                ..document_viewer::DisplaySettings::default()
            },
            show_line_numbers: false,
            code_block_theme: CodeBlockTheme::default(),
            show_heading_collapse: false,
            scroll_multiplier: 3,
        }
    }

    /// Sets the markdown code-block color theme.
    pub fn set_code_block_theme(&mut self, theme: CodeBlockTheme) -> bool {
        if self.code_block_theme != theme {
            self.code_block_theme = theme;
            true
        } else {
            false
        }
    }

    /// Sets the wheel-scroll multiplier.
    pub fn set_scroll_multiplier(&mut self, multiplier: usize) -> bool {
        if self.scroll_multiplier != multiplier {
            self.scroll_multiplier = multiplier;
            true
        } else {
            false
        }
    }

    /// Returns the current wheel-scroll multiplier.
    pub fn scroll_multiplier(&self) -> usize {
        self.scroll_multiplier
    }

    /// Enables or disables document-wide line numbers through shared viewer state.
    pub fn set_show_document_line_numbers(&mut self, show: bool) -> bool {
        if self.viewer.show_line_numbers != show {
            self.viewer.show_line_numbers = show;
            true
        } else {
            false
        }
    }

    /// Returns whether document-wide line numbers are enabled.
    pub fn show_document_line_numbers(&self) -> bool {
        self.viewer.show_line_numbers
    }

    /// Enables or disables heading collapse indicators.
    pub fn set_show_heading_collapse(&mut self, show: bool) -> bool {
        if self.show_heading_collapse != show {
            self.show_heading_collapse = show;
            true
        } else {
            false
        }
    }

    /// Enables or disables fenced code-block line numbers.
    pub fn set_show_line_numbers(&mut self, show: bool) -> bool {
        if self.show_line_numbers != show {
            self.show_line_numbers = show;
            true
        } else {
            false
        }
    }
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self::new()
    }
}
