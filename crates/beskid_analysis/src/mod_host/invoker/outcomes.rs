use crate::syntax::Spanned;

use super::super::types::ProgramItem;

/// Outcome from a `Collector.Collect` invocation. The MVP carries the `typeId` of the
/// contract that was invoked and any extra "scope narrowing" tokens it produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectorOutcome {
    pub type_id: String,
    pub narrowed_targets: Vec<String>,
}

/// Outcome from a `Generator.Generate` invocation: zero or more typed AST contributions
/// spliced directly into the host program by `merge::merge_generated_syntax`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratorOutcome {
    pub type_id: String,
    pub typed_items: Vec<Spanned<ProgramItem>>,
    pub code_outputs: Vec<super::super::generate_output::CodeGenerateOutput>,
}

/// Outcome from `Analyzer.Analyze` — diagnostics it wants the host to emit and quick-fixes
/// it wants surfaced to LSP code actions. Fixes carry a `diagnostic_index` into
/// `diagnostics` so the host can resolve the linked diagnostic without string matching.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalyzerOutcome {
    pub type_id: String,
    pub diagnostics: Vec<AnalyzerDiagnostic>,
    pub fixes: Vec<AnalyzerFix>,
}

/// One diagnostic emitted by an Analyzer contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalyzerDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: AnalyzerSeverity,
    /// Byte offset range in the entry source: `(start, end)`.
    /// When `None`, the host falls back to a whole-file span so the diagnostic is
    /// still surfaced (e.g. for analyzers that report project-wide issues).
    pub span: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnalyzerSeverity {
    Error,
    #[default]
    Warning,
    Note,
}

/// One quick-fix produced by an Analyzer contract. `diagnostic_index` indexes into the
/// enclosing [`AnalyzerOutcome::diagnostics`] slice. Edits reuse [`RewriteEdit`] — the
/// host mirror of `ModEdit` — so the same apply path serves rewriters and quick-fixes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalyzerFix {
    pub diagnostic_index: u32,
    pub title: String,
    pub edits: Vec<RewriteEdit>,
}

/// One text edit produced by a Rewriter contract. Edits are byte-offset ranges into
/// the entry source; the host applies them right-to-left to preserve earlier offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewriteEdit {
    /// Insert `text` at byte `offset`.
    Insert { offset: usize, text: String },
    /// Replace bytes `start..end` with `text`.
    Replace { start: usize, end: usize, text: String },
    /// Delete bytes `start..end`.
    Delete { start: usize, end: usize },
}

/// Outcome from `Rewriter.Rewrite` — carries the `typeId`, a count of fixes the
/// rewriter applied internally, and the text edits it wants the host to apply to
/// the entry source. The host applies `edits` right-to-left after all rewriters
/// have run (see `rewrite::apply_edits`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RewriterOutcome {
    pub type_id: String,
    pub applied_fix_count: u32,
    /// Text edits the rewriter wants applied to the source.
    pub edits: Vec<RewriteEdit>,
}
