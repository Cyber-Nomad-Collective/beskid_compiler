//! Line classification for normalized documents.

/// Describes how a normalized document line should be interpreted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DocumentLineKind {
    /// Plain prose or source text.
    #[default]
    Text,
    /// Heading-like line that can appear in an outline.
    Heading {
        /// Heading level.
        level: u8,
        /// Whether nested content is collapsed.
        collapsed: bool,
    },
    /// Source code line.
    Code,
    /// Visual separator line.
    Separator,
    /// Metadata line.
    Metadata,
    /// Empty line.
    Empty,
}
