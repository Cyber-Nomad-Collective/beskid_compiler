//! Normalized line type for document viewers.

use ratatui::text::Span;

use super::DocumentLineKind;

/// A single rendered line with stable source-line metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentLine {
    /// One-based source line number.
    pub source_line: usize,
    /// Styled spans to render for this line.
    pub spans: Vec<Span<'static>>,
    /// Semantic line classification.
    pub kind: DocumentLineKind,
}

impl DocumentLine {
    /// Creates a rendered document line from spans.
    pub fn new(source_line: usize, spans: Vec<Span<'static>>, kind: DocumentLineKind) -> Self {
        Self {
            source_line,
            spans,
            kind,
        }
    }

    /// Creates a plain text document line.
    pub fn plain(source_line: usize, text: impl Into<String>, kind: DocumentLineKind) -> Self {
        Self::new(source_line, vec![Span::raw(text.into())], kind)
    }

    /// Returns the unstyled line text.
    pub fn plain_text(&self) -> String {
        self.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }
}
