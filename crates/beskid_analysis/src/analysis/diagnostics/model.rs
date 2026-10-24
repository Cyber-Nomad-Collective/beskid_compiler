//! Parts of the one structured diagnostic: labelled related locations, fixes with applicability,
//! and tags.
//!
//! Every renderer (the CLI drawing, `--error-format=json`, the language server) reads these parts
//! from [`super::SemanticDiagnostic`]; none of them parses message text.

use miette::{NamedSource, SourceSpan};

use crate::syntax::SpanInfo;

/// How safely a [`Fix`] can be applied without a person reading it (rustc's definitions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Applicability {
    /// The fix is definitely what the author meant and can be applied mechanically.
    MachineApplicable,
    /// The fix may or may not be what the author meant; a person must check the result.
    MaybeIncorrect,
    /// The fix contains placeholder text that the author must replace.
    HasPlaceholders,
}

impl Applicability {
    /// Stable token used by `--error-format=json` and the language server's `data` payload.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MachineApplicable => "MachineApplicable",
            Self::MaybeIncorrect => "MaybeIncorrect",
            Self::HasPlaceholders => "HasPlaceholders",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "MachineApplicable" => Some(Self::MachineApplicable),
            "MaybeIncorrect" => Some(Self::MaybeIncorrect),
            "HasPlaceholders" => Some(Self::HasPlaceholders),
            _ => None,
        }
    }
}

/// Presentation tags an editor applies to the primary span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticTag {
    /// Unused or unreachable code; editors fade it out.
    Unnecessary,
    /// Use of a deprecated item; editors strike it through.
    Deprecated,
}

impl DiagnosticTag {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unnecessary => "unnecessary",
            Self::Deprecated => "deprecated",
        }
    }
}

/// A source other than the diagnostic's primary source (a related location in another file).
#[derive(Debug, Clone)]
pub struct RelatedSource {
    pub name: String,
    pub text: String,
}

/// A labelled place other than the primary span that explains the cause ("first declared here",
/// "assigned again"). `source` is `None` when the place is in the primary source.
#[derive(Debug, Clone)]
pub struct RelatedLocation {
    pub source: Option<RelatedSource>,
    pub span: SourceSpan,
    pub label: String,
}

impl RelatedLocation {
    /// Name of the file the location is in, given the diagnostic's primary source name.
    pub fn file_name<'a>(&'a self, primary: &'a str) -> &'a str {
        self.source.as_ref().map_or(primary, |source| source.name.as_str())
    }

    /// Text of the file the location is in, given the diagnostic's primary source text.
    pub fn file_text<'a>(&'a self, primary: &'a str) -> &'a str {
        self.source.as_ref().map_or(primary, |source| source.text.as_str())
    }
}

/// One replacement in the diagnostic's primary source. An empty span inserts `replacement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixEdit {
    pub span: SourceSpan,
    pub replacement: String,
}

impl FixEdit {
    /// Replace the bytes of `span` (an empty span inserts at `span.start`).
    pub fn new(span: SpanInfo, replacement: impl Into<String>) -> Self {
        let end = span.end.max(span.start);
        Self { span: SourceSpan::new(span.start.into(), end - span.start), replacement: replacement.into() }
    }

    /// Insert `text` before byte `at`.
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self { span: SourceSpan::new(at.into(), 0), replacement: text.into() }
    }

    /// Byte range `start..end` this edit replaces.
    pub fn range(&self) -> (usize, usize) {
        (self.span.offset(), self.span.offset() + self.span.len())
    }
}

/// A quick fix: a short imperative title ("Insert `move`"), its edits, and how safe it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub title: String,
    pub applicability: Applicability,
    pub edits: Vec<FixEdit>,
}

impl Fix {
    pub fn new(title: impl Into<String>, applicability: Applicability, edits: Vec<FixEdit>) -> Self {
        Self { title: title.into(), applicability, edits }
    }

    /// A fix with one edit that replaces `span` with `replacement`.
    pub fn replace(
        title: impl Into<String>,
        span: SpanInfo,
        replacement: impl Into<String>,
        applicability: Applicability,
    ) -> Self {
        Self::new(title, applicability, vec![FixEdit::new(span, replacement)])
    }

    /// Only a machine-applicable fix is offered as the preferred quick fix.
    pub fn is_preferred(&self) -> bool {
        self.applicability == Applicability::MachineApplicable
    }

    /// The title an editor shows: a fix that may be wrong says so.
    pub fn editor_title(&self) -> String {
        match self.applicability {
            Applicability::MachineApplicable => self.title.clone(),
            Applicability::MaybeIncorrect | Applicability::HasPlaceholders => {
                format!("{} (check the result)", self.title)
            }
        }
    }
}

/// 1-based line and column of `offset` in `source`; columns count Unicode scalar values.
pub fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut boundary = offset.min(source.len());
    while !source.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let before = &source[..boundary];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// Byte span `span` clamped into `source` (a zero-length span stays zero-length).
pub(crate) fn clamp_span(span: SpanInfo, source: &str) -> SourceSpan {
    let start = span.start.min(source.len());
    let end = span.end.max(start).min(source.len());
    SourceSpan::new(start.into(), end - start)
}

/// The text of a named source, for diagnostics that hold it as a miette [`NamedSource`].
pub(crate) fn named_source_text(source: &NamedSource<String>) -> &str {
    source.inner().as_str()
}
