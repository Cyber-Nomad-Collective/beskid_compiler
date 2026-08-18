//! Mod-host diagnostics for registration validation and contract scheduling.
//!
//! These diagnostics fire **before** `mod.collect` and abort scheduling per
//! `site/website/src/content/docs/platform-spec/compiler/compiler-mods/mod-host-bridge/`.
//! Codes follow the platform-spec **E1821–E1835** (load failures) and **E1851–E1870**
//! (cross-artifact / scheduling conflicts) bands. They do not carry Beskid source spans —
//! conflicts are anchored at the mod artifact descriptor or project manifest path.

use std::fmt;
use std::path::PathBuf;

use miette::{NamedSource, SourceSpan};

use crate::analysis::diagnostics::{SemanticDiagnostic, Severity};
use crate::syntax::SpanInfo;

use super::invoker::{AnalyzerDiagnostic, AnalyzerFix, AnalyzerOutcome, AnalyzerSeverity, RewriteEdit};

/// Kind of a [`SyntaxTextEdit`], mirroring `ModEdit.kind` (0=Insert, 1=Replace, 2=Delete).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxTextEditKind {
    Insert,
    Replace,
    Delete,
}

/// One text edit in a [`SyntaxFix`]. Byte-offset ranges into the entry source; the LSP
/// converts these to `TextEdit`s when surfacing a `QUICKFIX` code action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTextEdit {
    pub kind: SyntaxTextEditKind,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// One quick-fix produced by a mod `Analyzer` contract, shaped for LSP code-action
/// surfacing. `source` is the mod-origin tag (`"beskid:mod:<type_id>"`) and
/// `diagnostic_code` links the fix to the [`SyntaxDiagnostic`](crate::...) it addresses.
///
/// Defined in `beskid_analysis` (the prepare spine returns it) and re-exported from the
/// LSP `session::store` module so the LSP stores the same single implementation on
/// `Document.syntax_fixes` (DRY — no duplicate LSP-side `SyntaxFix`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxFix {
    /// Mod-origin tag, e.g. `"beskid:mod:ModA.Check"`. Matches `SyntaxDiagnostic.source`.
    pub source: String,
    /// Code of the diagnostic this fix addresses (links to `SyntaxDiagnostic.code`).
    pub diagnostic_code: String,
    /// Human-readable title shown in the LSP code-action menu.
    pub title: String,
    /// Edits to apply when the fix is accepted.
    pub edits: Vec<SyntaxTextEdit>,
}

/// A single mod-host issue surfaced by registration validation or scheduling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModHostIssue {
    /// E1828 — `entrySymbol` referenced by a registration is empty / missing in the
    /// descriptor.
    MissingEntrySymbol { package_id: String, contract_id: String, type_id: String, descriptor: PathBuf },
    /// E1829 — Duplicate `(contractId, typeId)` registration in one artifact.
    DuplicateRegistrationInArtifact { package_id: String, contract_id: String, type_id: String, descriptor: PathBuf },
    /// E1830 — `registrations` empty but mod package declared required contracts.
    EmptyRegistrationsForRequiredMod { package_id: String, manifest: PathBuf },
    /// E1831 — Required capability missing for one or more registrations.
    MissingCapability { package_id: String, contract_id: String, capability: String, manifest: PathBuf },
    /// E1832 — `maxGeneratorRounds` exceeded during host mod.generate scheduling.
    MaxGeneratorRoundsExceeded { limit: u32 },
    /// E1851 — Same `(contractId, typeId)` is provided by multiple mod artifacts. Hosts
    /// must reject this rather than picking a winner non-deterministically.
    ConflictingRegistrationAcrossArtifacts { contract_id: String, type_id: String, package_ids: Vec<String> },
    /// E1852 — Two distinct artifacts export the **same `entrySymbol`** for different
    /// `(contractId, typeId)` tuples; loader cannot disambiguate native exports.
    DuplicateEntrySymbolAcrossArtifacts { entry_symbol: String, package_ids: Vec<String> },
    /// E1853 — Unknown `contractId` value in a registration (not a known SDK contract).
    UnknownContractId { package_id: String, contract_id: String, descriptor: PathBuf },
    /// E1854 — `Rewriter` registration without an attached `Analyzer` registration in
    /// the same artifact. The host requires analyzer-driven registration of rewrites
    /// per `compiler-mod-sdk`.
    RewriterWithoutAnalyzer { package_id: String, type_id: String, descriptor: PathBuf },
    /// E1855 — Catch-all scheduling-stage conflict / failure when no narrower code
    /// from the **E1851–E1870** band applies.
    SchedulingFailure { package_id: String, message: String },
}

impl ModHostIssue {
    pub fn code(&self) -> &'static str {
        match self {
            ModHostIssue::MissingEntrySymbol { .. } => "E1828",
            ModHostIssue::DuplicateRegistrationInArtifact { .. } => "E1829",
            ModHostIssue::EmptyRegistrationsForRequiredMod { .. } => "E1830",
            ModHostIssue::MissingCapability { .. } => "E1831",
            ModHostIssue::MaxGeneratorRoundsExceeded { .. } => "E1832",
            ModHostIssue::ConflictingRegistrationAcrossArtifacts { .. } => "E1851",
            ModHostIssue::DuplicateEntrySymbolAcrossArtifacts { .. } => "E1852",
            ModHostIssue::UnknownContractId { .. } => "E1853",
            ModHostIssue::RewriterWithoutAnalyzer { .. } => "E1854",
            ModHostIssue::SchedulingFailure { .. } => "E1855",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ModHostIssue::MissingEntrySymbol { package_id, contract_id, type_id, .. } => {
                format!("mod `{package_id}` registration `{contract_id}` for type `{type_id}` has empty `entrySymbol`")
            }
            ModHostIssue::DuplicateRegistrationInArtifact { package_id, contract_id, type_id, .. } => format!(
                "mod `{package_id}` declares duplicate registration `({contract_id}, {type_id})` in one artifact"
            ),
            ModHostIssue::EmptyRegistrationsForRequiredMod { package_id, .. } => format!(
                "mod `{package_id}` declares required contract capabilities but its artifact `registrations` array is empty"
            ),
            ModHostIssue::MissingCapability { package_id, contract_id, capability, .. } => format!(
                "mod `{package_id}` registers `{contract_id}` but is missing required capability `{capability}`"
            ),
            ModHostIssue::MaxGeneratorRoundsExceeded { limit } => {
                format!("mod host exceeded `maxGeneratorRounds` limit of {limit} while merging generated syntax")
            }
            ModHostIssue::ConflictingRegistrationAcrossArtifacts { contract_id, type_id, package_ids } => format!(
                "registration `({contract_id}, {type_id})` is provided by multiple mod artifacts: {}",
                package_ids.join(", ")
            ),
            ModHostIssue::DuplicateEntrySymbolAcrossArtifacts { entry_symbol, package_ids } => format!(
                "entry symbol `{entry_symbol}` is exported by multiple mod artifacts: {}",
                package_ids.join(", ")
            ),
            ModHostIssue::UnknownContractId { package_id, contract_id, .. } => format!(
                "mod `{package_id}` registers unknown contract id `{contract_id}` (not a recognized SDK contract)"
            ),
            ModHostIssue::RewriterWithoutAnalyzer { package_id, type_id, .. } => {
                format!("mod `{package_id}` registers Rewriter `{type_id}` but provides no Analyzer to drive it")
            }
            ModHostIssue::SchedulingFailure { package_id, message } => {
                format!("mod `{package_id}` scheduling failed: {message}")
            }
        }
    }
}

impl fmt::Display for ModHostIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

/// Aggregate error returned by `mod.load` / pre-collect validation.
///
/// Multiple mods can fail in one pass; this carries every issue so the host emits all
/// of them deterministically before `mod.collect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModHostDiagnostics {
    pub issues: Vec<ModHostIssue>,
}

impl ModHostDiagnostics {
    pub fn new(issues: Vec<ModHostIssue>) -> Self {
        Self { issues }
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn codes(&self) -> Vec<&'static str> {
        self.issues.iter().map(ModHostIssue::code).collect()
    }
}

impl fmt::Display for ModHostDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "mod host scheduling aborted before `mod.collect`:")?;
        for issue in &self.issues {
            writeln!(f, "  {issue}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ModHostDiagnostics {}

/// Bridge mod analyzer diagnostics into the semantic diagnostic stream during prepare.
///
/// When the analyzer provides a byte span (`diagnostic.span`), the semantic
/// diagnostic is anchored at that range so LSP code actions can resolve against
/// the real source location. When the span is `None`, the host falls back to a
/// whole-file span so the diagnostic is still surfaced.
pub fn analyzer_diagnostic_to_semantic(
    diagnostic: &AnalyzerDiagnostic,
    type_id: &str,
    source_name: &str,
    source: &str,
) -> SemanticDiagnostic {
    let severity = match diagnostic.severity {
        AnalyzerSeverity::Error => Severity::Error,
        AnalyzerSeverity::Warning => Severity::Warning,
        AnalyzerSeverity::Note => Severity::Note,
    };
    let span = match diagnostic.span {
        Some((start, end)) => SpanInfo::from_byte_range_in_source(source, start, end),
        None => SpanInfo::from_byte_range_in_source(source, 0, source.len().max(1)),
    };
    SemanticDiagnostic {
        src: NamedSource::new(source_name, source.to_owned()),
        span: SourceSpan::new(span.start.into(), (span.end - span.start).max(1)),
        message: format!("{} ({})", diagnostic.message, type_id),
        label: diagnostic.code.clone(),
        help: Some(format!("mod analyzer contract `{type_id}`")),
        code: Some(diagnostic.code.clone()),
        origin: Some(format!("beskid:mod:{type_id}")),
        severity,
    }
}

/// Map a mod `Analyzer` quick-fix into the LSP-facing [`SyntaxFix`] shape.
///
/// `source` is the mod-origin tag (`format!("beskid:mod:{}", outcome.type_id)`) the caller
/// has already computed. The linked diagnostic is resolved via `fix.diagnostic_index`
/// into `outcome.diagnostics`; if the index is out of range the fix is dropped
/// (fail-closed — mirrors `unmarshal_fixes` in `native.rs`). `RewriteEdit` →
/// [`SyntaxTextEdit`] is a direct enum mirror.
pub fn analyzer_fix_to_syntax_fix(fix: &AnalyzerFix, outcome: &AnalyzerOutcome, source: &str) -> Option<SyntaxFix> {
    let diagnostic = outcome.diagnostics.get(fix.diagnostic_index as usize)?;
    let edits = fix.edits.iter().map(rewrite_edit_to_syntax_text_edit).collect();
    Some(SyntaxFix {
        source: source.to_owned(),
        diagnostic_code: diagnostic.code.clone(),
        title: fix.title.clone(),
        edits,
    })
}

fn rewrite_edit_to_syntax_text_edit(edit: &RewriteEdit) -> SyntaxTextEdit {
    match edit {
        RewriteEdit::Insert { offset, text } => {
            SyntaxTextEdit { kind: SyntaxTextEditKind::Insert, start: *offset, end: *offset, text: text.clone() }
        }
        RewriteEdit::Replace { start, end, text } => {
            SyntaxTextEdit { kind: SyntaxTextEditKind::Replace, start: *start, end: *end, text: text.clone() }
        }
        RewriteEdit::Delete { start, end } => {
            SyntaxTextEdit { kind: SyntaxTextEditKind::Delete, start: *start, end: *end, text: String::new() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_in_artifact_uses_e1829() {
        let issue = ModHostIssue::DuplicateRegistrationInArtifact {
            package_id: "ModA".to_owned(),
            contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
            type_id: "ModA.Emit".to_owned(),
            descriptor: PathBuf::from("/tmp/desc.json"),
        };
        assert_eq!(issue.code(), "E1829");
        assert!(issue.message().contains("ModA.Emit"));
    }

    #[test]
    fn cross_artifact_conflict_uses_e1851() {
        let issue = ModHostIssue::ConflictingRegistrationAcrossArtifacts {
            contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
            type_id: "Shared.Type".to_owned(),
            package_ids: vec!["ModA".to_owned(), "ModB".to_owned()],
        };
        assert_eq!(issue.code(), "E1851");
    }

    #[test]
    fn diagnostics_aggregate_codes() {
        let diag = ModHostDiagnostics::new(vec![
            ModHostIssue::DuplicateRegistrationInArtifact {
                package_id: "ModA".to_owned(),
                contract_id: "Beskid.Compiler.Collect.Generator".to_owned(),
                type_id: "T".to_owned(),
                descriptor: PathBuf::from("/tmp/a"),
            },
            ModHostIssue::ConflictingRegistrationAcrossArtifacts {
                contract_id: "Beskid.Compiler.Collect.Analyzer".to_owned(),
                type_id: "U".to_owned(),
                package_ids: vec!["ModA".into(), "ModB".into()],
            },
        ]);
        assert_eq!(diag.codes(), vec!["E1829", "E1851"]);
        assert!(!diag.is_empty());
    }

    #[test]
    fn analyzer_diagnostic_with_span_anchors_at_range() {
        let source = "unit Main() { return; }\n";
        let diagnostic = AnalyzerDiagnostic {
            code: "MOD0001".to_owned(),
            message: "point issue".to_owned(),
            severity: AnalyzerSeverity::Warning,
            span: Some((5, 9)),
        };
        let semantic = analyzer_diagnostic_to_semantic(&diagnostic, "ModA.Check", "Main.bd", source);
        // Span 5..9 maps to "Main" inside the source.
        assert_eq!(semantic.span.offset(), 5);
        assert!(!semantic.span.is_empty());
        assert!(semantic.message.contains("ModA.Check"));
        assert_eq!(semantic.origin.as_deref(), Some("beskid:mod:ModA.Check"));
    }

    #[test]
    fn analyzer_diagnostic_without_span_falls_back_to_whole_file() {
        let source = "unit Main() { return; }\n";
        let diagnostic = AnalyzerDiagnostic {
            code: "MOD0002".to_owned(),
            message: "project-wide issue".to_owned(),
            severity: AnalyzerSeverity::Note,
            span: None,
        };
        let semantic = analyzer_diagnostic_to_semantic(&diagnostic, "ModA.Check", "Main.bd", source);
        // Whole-file fallback starts at offset 0.
        assert_eq!(semantic.span.offset(), 0);
        assert_eq!(semantic.origin.as_deref(), Some("beskid:mod:ModA.Check"));
    }
}
