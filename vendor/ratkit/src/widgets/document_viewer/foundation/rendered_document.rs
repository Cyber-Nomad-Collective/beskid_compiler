//! Complete normalized rendered document.

use super::{DocumentLine, DocumentOutlineItem};

/// A rendered document ready for viewport rendering.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedDocument {
    /// Renderable lines.
    pub lines: Vec<DocumentLine>,
    /// Optional outline entries.
    pub outline: Vec<DocumentOutlineItem>,
}

impl RenderedDocument {
    /// Creates a rendered document from lines and outline entries.
    pub fn new(lines: Vec<DocumentLine>, outline: Vec<DocumentOutlineItem>) -> Self {
        Self { lines, outline }
    }

    /// Returns the number of renderable lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Returns true when the document has no lines.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}
