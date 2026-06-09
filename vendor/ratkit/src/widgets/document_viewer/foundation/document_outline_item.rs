//! Outline entry type for normalized documents.

/// A navigable symbol or section in a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentOutlineItem {
    /// Display title for the outline row.
    pub title: String,
    /// One-based source line number.
    pub line: usize,
    /// Nesting level, where zero is top-level.
    pub level: usize,
    /// Symbol or section kind.
    pub kind: String,
}

impl DocumentOutlineItem {
    /// Creates a new outline item.
    pub fn new(
        title: impl Into<String>,
        line: usize,
        level: usize,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            line,
            level,
            kind: kind.into(),
        }
    }
}
