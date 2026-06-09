//! Events emitted by the code widget.

/// Outcome of code-widget input handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeEvent {
    /// No visible state changed.
    None,
    /// Viewport focus moved to a zero-based line.
    Navigated {
        /// Zero-based focused line.
        line: usize,
    },
    /// Text was copied into widget state.
    Copied {
        /// Copied source text.
        text: String,
    },
    /// Selection changed.
    SelectionChanged,
    /// Outline TOC hover state changed.
    OutlineHoverChanged,
}
