//! Source line model for code-widget parsing.

/// A raw source line with one-based line metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLine {
    /// One-based source line number.
    pub number: usize,
    /// Raw source text.
    pub text: String,
}

impl CodeLine {
    /// Creates a source line model.
    pub fn new(number: usize, text: impl Into<String>) -> Self {
        Self {
            number,
            text: text.into(),
        }
    }
}
