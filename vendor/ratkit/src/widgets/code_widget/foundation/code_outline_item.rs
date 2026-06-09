//! Code-specific outline entry.

use crate::widgets::document_viewer::DocumentOutlineItem;

/// A lightweight source symbol extracted for the outline pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeOutlineItem {
    /// Symbol name.
    pub name: String,
    /// One-based source line number.
    pub line: usize,
    /// Nesting level derived from indentation or language structure.
    pub level: usize,
    /// Symbol kind.
    pub kind: String,
}

impl CodeOutlineItem {
    /// Creates a code outline item.
    pub fn new(
        name: impl Into<String>,
        line: usize,
        level: usize,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            line,
            level,
            kind: kind.into(),
        }
    }

    /// Converts the code outline item into the shared outline model.
    pub fn into_document_item(self) -> DocumentOutlineItem {
        DocumentOutlineItem::new(self.name, self.line, self.level, self.kind)
    }
}
