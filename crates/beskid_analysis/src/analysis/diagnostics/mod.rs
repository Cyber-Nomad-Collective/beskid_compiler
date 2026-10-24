//! The one structured diagnostic, [`SemanticDiagnostic`], and helpers for codes, spans and
//! severity.
//!
//! A diagnostic names one cause: a one-sentence `message`, a primary span anchored where the fix
//! goes with its `label`, labelled [`RelatedLocation`]s for the other places involved, optional
//! `help`, [`Fix`]es with [`Applicability`], and [`DiagnosticTag`]s. Renderers read these parts:
//! the CLI draws them with miette, `--error-format=json` serialises them ([`json`]), and the
//! language server maps them to LSP diagnostics. Before rendering, spans in materialized copies
//! are mapped back to the authored source ([`source_map`]) and duplicates are dropped
//! ([`dedupe`]).

pub mod dedupe;
pub mod model;

use std::fmt;

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode, SourceSpan};

use crate::syntax::SpanInfo;

pub use dedupe::dedupe_diagnostics;
pub use model::{Applicability, DiagnosticTag, Fix, FixEdit, RelatedLocation, RelatedSource, line_column};

/// Diagnostic band for rules and parse/project adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    /// Stable lower-case token (`error`, `warning`, `note`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

/// One issue anchored in source: the structured diagnostic every renderer reads.
#[derive(Debug, Clone)]
pub struct SemanticDiagnostic {
    /// The primary source (name and full text).
    pub src: NamedSource<String>,
    /// The primary span, anchored where the fix goes.
    pub span: SourceSpan,
    /// One sentence naming the cause; no paths, snippets or grammar rule names.
    pub message: String,
    /// Short label of the primary span.
    pub label: String,
    /// Optional help sentence.
    pub help: Option<String>,
    pub code: Option<String>,
    /// Origin tag for routing. `None` = compiler-origin diagnostic; `Some("beskid:mod:<type_id>")`
    /// = mod-origin diagnostic produced by a native `Analyzer` contract. LSP uses this to route
    /// code actions and the prepare spine uses it to attribute build failures.
    pub origin: Option<String>,
    pub severity: Severity,
    /// Other places involved in the cause, each with a short label.
    pub related: Vec<RelatedLocation>,
    /// Quick fixes, each with its applicability.
    pub fixes: Vec<Fix>,
    /// Presentation tags (unnecessary, deprecated).
    pub tags: Vec<DiagnosticTag>,
}

impl SemanticDiagnostic {
    /// Name of the primary source.
    pub fn file_name(&self) -> &str {
        self.src.name()
    }

    /// Text of the primary source.
    pub fn source_text(&self) -> &str {
        model::named_source_text(&self.src)
    }

    /// Byte range `start..end` of the primary span.
    pub fn range(&self) -> (usize, usize) {
        (self.span.offset(), self.span.offset() + self.span.len())
    }

    /// Add a labelled related location in the primary source.
    pub fn with_related(mut self, span: SpanInfo, label: impl Into<String>) -> Self {
        let span = model::clamp_span(span, self.source_text());
        self.related.push(RelatedLocation { source: None, span, label: label.into() });
        self
    }

    /// Add a labelled related location in another source.
    pub fn with_related_in(
        mut self,
        source_name: impl Into<String>,
        source_text: impl Into<String>,
        span: SpanInfo,
        label: impl Into<String>,
    ) -> Self {
        let source = RelatedSource { name: source_name.into(), text: source_text.into() };
        let span = model::clamp_span(span, &source.text);
        let source = (source.name != self.file_name()).then_some(source);
        self.related.push(RelatedLocation { source, span, label: label.into() });
        self
    }

    /// Add a quick fix. Edits outside the primary source are dropped (fail closed).
    pub fn with_fix(mut self, fix: Fix) -> Self {
        let len = self.source_text().len();
        if fix.edits.iter().all(|edit| edit.range().1 <= len) {
            self.fixes.push(fix);
        }
        self
    }

    /// Add a tag (once).
    pub fn with_tag(mut self, tag: DiagnosticTag) -> Self {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_origin(mut self, origin: impl Into<String>) -> Self {
        self.origin = Some(origin.into());
        self
    }
}

impl fmt::Display for SemanticDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SemanticDiagnostic {}

/// The CLI drawing: the primary label, every related location in the primary source as a
/// secondary label, and help. Related locations in other files and the fix titles are listed in
/// the help block, since miette draws one source per report.
impl Diagnostic for SemanticDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.code.as_ref().map(|code| Box::new(code) as Box<dyn fmt::Display>)
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(match self.severity {
            Severity::Error => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
            Severity::Note => miette::Severity::Advice,
        })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let mut lines: Vec<String> = self.help.iter().cloned().collect();
        for related in self.related.iter().filter(|related| related.source.is_some()) {
            let text = related.file_text(self.source_text());
            let (line, column) = line_column(text, related.span.offset());
            lines.push(format!("{}: {}:{line}:{column}", related.label, related.file_name(self.file_name())));
        }
        for fix in &self.fixes {
            lines.push(format!("fix: {}", fix.editor_title()));
        }
        (!lines.is_empty()).then(|| Box::new(lines.join("\n")) as Box<dyn fmt::Display>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(&self.src)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let primary = LabeledSpan::new_primary_with_span(Some(self.label.clone()), self.span);
        let related = self
            .related
            .iter()
            .filter(|related| related.source.is_none())
            .map(|related| LabeledSpan::new_with_span(Some(related.label.clone()), related.span));
        Some(Box::new(std::iter::once(primary).chain(related)))
    }
}

/// Wraps a [`SemanticDiagnostic`] so it can be stored as the root payload of an [`anyhow::Error`]
/// while remaining downcastable for CLI / tooling that want a structured [`miette::Report`].
#[derive(Debug, Clone)]
pub struct MietteReportError(SemanticDiagnostic);

impl MietteReportError {
    pub fn new(diagnostic: SemanticDiagnostic) -> Self {
        Self(diagnostic)
    }

    pub fn diagnostic(&self) -> &SemanticDiagnostic {
        &self.0
    }

    pub fn into_diagnostic(self) -> SemanticDiagnostic {
        self.0
    }
}

impl fmt::Display for MietteReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.message)
    }
}

impl std::error::Error for MietteReportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

pub fn span_to_sourcespan(span: SpanInfo) -> SourceSpan {
    let len = span.end.saturating_sub(span.start).max(1);
    SourceSpan::new(span.start.into(), len)
}

/// The label span for `span` inside `source`. A span that lies past the end of the source
/// (generated syntax is re-spanned into a synthetic region after the host unit) points at the
/// source's last byte instead: miette cannot render a label outside the text it was given.
pub fn span_to_sourcespan_within(span: SpanInfo, source: &str) -> SourceSpan {
    if source.is_empty() || span.start >= source.len() {
        return SourceSpan::new(source.len().saturating_sub(1).into(), source.len().min(1));
    }
    let len = span.end.saturating_sub(span.start).max(1).min(source.len() - span.start);
    SourceSpan::new(span.start.into(), len)
}

pub fn make_diagnostic(
    source_name: &str,
    source: &str,
    span: SpanInfo,
    message: impl Into<String>,
    label: impl Into<String>,
    help: Option<String>,
    code: Option<String>,
    severity: Severity,
) -> SemanticDiagnostic {
    let tags = code.as_deref().map(crate::analysis::diagnostic_codes::default_tags).unwrap_or_default().to_vec();
    SemanticDiagnostic {
        src: NamedSource::new(source_name, source.to_string()),
        span: span_to_sourcespan_within(span, source),
        message: message.into(),
        label: label.into(),
        help,
        code,
        origin: None,
        severity,
        related: Vec::new(),
        fixes: Vec::new(),
        tags,
    }
}

#[macro_export]
macro_rules! diag {
    ($ctx:expr, $span:expr, $code:expr, $message:expr $(, label = $label:expr)? $(, help = $help:expr)? $(, severity = $severity:expr)? ) => {{
        let label = $crate::diag!(@label $( $label )?);
        let help = $crate::diag!(@help $( $help )?);
        let severity = $crate::diag!(@severity $( $severity )?);
        let diagnostic = $crate::analysis::diagnostics::make_diagnostic(
            $ctx.source_name(),
            $ctx.source(),
            $span,
            $message,
            label,
            help,
            Some($code.to_string()),
            severity,
        );
        $ctx.emit(diagnostic);
    }};
    (@label $label:expr) => { $label };
    (@label) => { "here" };
    (@help $help:expr) => { Some($help.to_string()) };
    (@help) => { None };
    (@severity $severity:expr) => { $severity };
    (@severity) => { $crate::analysis::diagnostics::Severity::Error };
}

#[cfg(test)]
mod miette_report_error_tests {
    use super::*;

    #[test]
    fn miette_report_error_anyhow_roundtrip() {
        let diagnostic = make_diagnostic(
            "test.bd",
            "hello",
            SpanInfo { start: 0, end: 1, line_col_start: (1, 1), line_col_end: (1, 2) },
            "example diagnostic",
            "here",
            None,
            Some("E9999".to_string()),
            Severity::Error,
        );
        let wrapped = MietteReportError::new(diagnostic.clone());
        let anyhow_err = anyhow::Error::new(wrapped);
        let downcast =
            anyhow_err.downcast_ref::<MietteReportError>().expect("MietteReportError should round-trip through anyhow");
        assert_eq!(downcast.diagnostic().message, diagnostic.message);
        assert_eq!(downcast.diagnostic().code.as_deref(), diagnostic.code.as_deref());
    }
}

#[cfg(test)]
mod model_tests {
    use super::*;

    const SOURCE: &str = "i32 Main() {\n    i32 n = 1;\n    n = 2;\n    return n;\n}\n";

    fn span(start: usize, end: usize) -> SpanInfo {
        SpanInfo { start, end, line_col_start: (1, 1), line_col_end: (1, 1) }
    }

    fn immutable() -> SemanticDiagnostic {
        let declaration = SOURCE.find("i32 n").unwrap();
        let assignment = SOURCE.find("n = 2").unwrap();
        make_diagnostic(
            "Src/Main.bd",
            SOURCE,
            span(declaration + 4, declaration + 5),
            "`n` is assigned more than once but is not mutable.",
            "declared here",
            Some("declare `n` with `mut`".into()),
            Some("E1214".into()),
            Severity::Error,
        )
        .with_related(span(assignment, assignment + 1), "assigned again")
        .with_fix(Fix::new(
            "Make `n` mutable",
            Applicability::MachineApplicable,
            vec![FixEdit::insert(declaration + 4, "mut ")],
        ))
    }

    fn render(diagnostic: &SemanticDiagnostic) -> String {
        let mut out = String::new();
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
            .render_report(&mut out, diagnostic)
            .expect("render");
        out
    }

    #[test]
    fn builder_records_related_locations_fixes_and_tags() {
        let diagnostic = immutable().with_tag(DiagnosticTag::Deprecated).with_tag(DiagnosticTag::Deprecated);
        assert_eq!(diagnostic.related.len(), 1);
        assert_eq!(diagnostic.related[0].label, "assigned again");
        assert!(diagnostic.related[0].source.is_none());
        assert_eq!(diagnostic.fixes.len(), 1);
        assert!(diagnostic.fixes[0].is_preferred());
        assert_eq!(diagnostic.tags, [DiagnosticTag::Deprecated]);
    }

    #[test]
    fn fix_edits_outside_the_primary_source_are_dropped() {
        let diagnostic = immutable().with_fix(Fix::new(
            "Out of range",
            Applicability::MachineApplicable,
            vec![FixEdit::insert(SOURCE.len() + 10, "x")],
        ));
        assert_eq!(diagnostic.fixes.len(), 1, "only the in-range fix is kept");
    }

    #[test]
    fn related_location_in_the_same_file_is_not_foreign() {
        let diagnostic = immutable().with_related_in("Src/Main.bd", SOURCE, span(0, 3), "return type");
        assert!(diagnostic.related[1].source.is_none());
        let foreign = immutable().with_related_in("Src/Other.bd", "i32 X;", span(4, 5), "first declared here");
        assert_eq!(foreign.related[1].file_name(foreign.file_name()), "Src/Other.bd");
    }

    #[test]
    fn non_machine_applicable_fix_titles_say_to_check_the_result() {
        let fix = Fix::replace("Use `Text`", span(0, 3), "Text", Applicability::MaybeIncorrect);
        assert!(!fix.is_preferred());
        assert_eq!(fix.editor_title(), "Use `Text` (check the result)");
        assert_eq!(Applicability::parse(Applicability::HasPlaceholders.as_str()), Some(Applicability::HasPlaceholders));
    }

    #[test]
    fn registry_tags_are_applied_from_the_code() {
        let unused = make_diagnostic(
            "Main.bd",
            SOURCE,
            span(0, 3),
            "unused import",
            "here",
            None,
            Some("W1503".into()),
            Severity::Warning,
        );
        assert_eq!(unused.tags, [DiagnosticTag::Unnecessary]);
        assert!(immutable().tags.is_empty());
    }

    #[test]
    fn cli_drawing_shows_labels_help_and_fixes() {
        let foreign = immutable().with_related_in("Src/Other.bd", "a\nb", span(2, 3), "first declared here");
        let rendered = render(&foreign);
        assert!(rendered.contains("E1214"), "{rendered}");
        assert!(rendered.contains("declared here"), "{rendered}");
        assert!(rendered.contains("assigned again"), "{rendered}");
        assert!(rendered.contains("declare `n` with `mut`"), "{rendered}");
        assert!(rendered.contains("first declared here: Src/Other.bd:2:1"), "{rendered}");
        assert!(rendered.contains("fix: Make `n` mutable"), "{rendered}");
    }

    #[test]
    fn line_column_counts_scalar_values() {
        assert_eq!(line_column("ab\nżc", 5), (2, 2));
        assert_eq!(line_column("ab", 0), (1, 1));
    }
}
