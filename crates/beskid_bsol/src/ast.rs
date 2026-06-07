//! Generic Bsol abstract syntax tree.

/// A parsed Bsol document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolDocument {
    pub blocks: Vec<BsolBlock>,
}

/// One block in a Bsol document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolBlock {
    pub span: BsolSpan,
    pub kind: String,
    pub label: Option<BsolQuotedString>,
    /// When `@schemaless` is present, inner `{ ... }` text is captured verbatim (no nested parse).
    pub schemaless_body: Option<String>,
    pub items: Vec<BsolItem>,
}

/// Body item: assignment or nested block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolItem {
    Assignment(BsolAssignment),
    Block(BsolBlock),
}

/// `key = value` assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolAssignment {
    pub span: BsolSpan,
    pub key: String,
    pub value: BsolValue,
}

/// Right-hand side of an assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolValue {
    QuotedString(BsolQuotedString),
    Ident(String),
    BracketList(BsolBracketList),
}

/// Double-quoted string literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolQuotedString {
    pub span: BsolSpan,
    pub value: String,
}

/// Bracket list `[a, b, "c"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolBracketList {
    pub span: BsolSpan,
    pub items: Vec<BsolListItem>,
}

/// One element of a bracket list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolListItem {
    Default,
    QuotedString(BsolQuotedString),
    Ident(String),
}

/// UTF-8 source span with 1-based line index for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BsolSpan {
    pub start: usize,
    pub end: usize,
    pub line: usize,
}

impl BsolSpan {
    pub fn from_pest(span: pest::Span<'_>, source: &str) -> Self {
        let start = span.start();
        let end = span.end();
        let line = source[..start.min(source.len())].lines().count().max(1);
        Self { start, end, line }
    }
}

impl BsolQuotedString {
    pub fn new(span: BsolSpan, raw: &str) -> Self {
        let value = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw)
            .to_string();
        Self { span, value }
    }
}
