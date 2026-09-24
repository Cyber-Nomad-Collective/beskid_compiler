//! Diagnostic deduplication, analyzer bridging, input resolution, and typed fingerprints.

use crate::AnalysisOptions;
use crate::analysis::SemanticDiagnostic;
use crate::analysis::rules::{RuleContext, resolve, types};
use crate::mod_host::SyntaxFix;
use crate::mod_host::diagnostics::{analyzer_diagnostic_to_semantic, analyzer_fix_to_syntax_fix};
use crate::projects::{CompilePlan, PreparedProjectWorkspace, ProgramAssembly, SourceUnit};

use super::super::input::ResolvedInput;
use super::super::semantic_facts::SemanticFactsError;

/// Drop exact-duplicate diagnostics, keeping the first occurrence.
///
/// A genuine invalid `?` target is independently detected twice on the fact-authority path: once
/// by the authority callback (driven by the typed `try_expression_fact`) and again by
/// `resolve_and_type_program_with_assembly`'s own full type check, which runs the same
/// `is_result_shaped_enum` judgment through `type_try_expression` and, on failure, surfaces
/// `TypeError::InvalidTryTarget` via `semantic_facts_errors_to_diagnostics`. Both sites are
/// correct in isolation; only the caller-facing duplicate needs suppressing, so this dedups by
/// exact (source, span, code, message) rather than special-casing E1222 alone. The source name is
/// part of the identity: a dependency-unit finding at the same offset as an entry diagnostic is a
/// different diagnostic.
pub(super) fn dedupe_diagnostics(diagnostics: Vec<SemanticDiagnostic>) -> Vec<SemanticDiagnostic> {
    let mut seen: std::collections::HashSet<(String, usize, usize, Option<String>, String)> =
        std::collections::HashSet::new();
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            let key = (
                diagnostic.src.name().to_string(),
                diagnostic.span.offset(),
                diagnostic.span.len(),
                diagnostic.code.clone(),
                diagnostic.message.clone(),
            );
            seen.insert(key)
        })
        .collect()
}

/// Render semantic-fact findings against the unit that owns each one: the entry unit renders
/// against the spine's entry source, every other unit against its own assembled source. A
/// finding naming a unit outside the assembly fails closed.
pub(super) fn fact_findings_to_diagnostics(
    assembly: &ProgramAssembly,
    entry_source: &str,
    findings: Vec<super::SemanticFactFinding>,
) -> anyhow::Result<Vec<SemanticDiagnostic>> {
    findings
        .into_iter()
        .map(|finding| {
            let unit = assembly.units.get(finding.unit).ok_or_else(|| {
                anyhow::anyhow!("semantic fact finding names unit {} outside the prepared assembly", finding.unit)
            })?;
            let source = if finding.unit == assembly.entry_index { entry_source } else { unit.source.as_str() };
            Ok(crate::analysis::diagnostics::make_diagnostic(
                &unit.logical_name,
                source,
                finding.span,
                finding.kind.message(),
                finding.kind.label(),
                finding.kind.help(),
                Some(finding.kind.code().to_string()),
                finding.kind.severity(),
            ))
        })
        .collect()
}

/// Append semantic-fact diagnostics to the spine's collected diagnostics, dropping a fact
/// diagnostic that a World A rule already reported for the same code in the same unit at an
/// overlapping span (the entry unit is judged by both; the two authorities may anchor the same
/// error on slightly different nodes).
pub(super) fn merge_fact_diagnostics(collected: &mut Vec<SemanticDiagnostic>, facts: Vec<SemanticDiagnostic>) {
    let existing = collected.len();
    for fact in facts {
        let overlaps = |span: &miette::SourceSpan| {
            let (start, end) = (span.offset(), span.offset() + span.len().max(1));
            let (fact_start, fact_end) = (fact.span.offset(), fact.span.offset() + fact.span.len().max(1));
            start < fact_end && fact_start < end
        };
        let reported = collected[..existing].iter().any(|diagnostic| {
            diagnostic.code == fact.code && diagnostic.src.name() == fact.src.name() && overlaps(&diagnostic.span)
        });
        if !reported {
            collected.push(fact);
        }
    }
}

/// Build a [`ResolvedInput`] from paths for analyze/LSP when only a compile plan is available.
pub fn resolved_input_from_plan(
    source_path: std::path::PathBuf,
    source: String,
    compile_plan: CompilePlan,
    prepared_workspace: Option<PreparedProjectWorkspace>,
    assembly: Option<ProgramAssembly>,
) -> ResolvedInput {
    ResolvedInput {
        source_path,
        source,
        compile_plan: Some(compile_plan),
        prepared_workspace,
        workspace_summary: None,
        assembly,
    }
}

/// Map mod `Analyzer` outcomes into the semantic diagnostic stream.
///
/// Each `AnalyzerDiagnostic` is bridged via [`analyzer_diagnostic_to_semantic`], which tags
/// the diagnostic with `origin: Some("beskid:mod:<type_id>")` so LSP can route code actions
/// and the prepare spine can attribute build failures to mod contracts.
pub(super) fn collect_analyzer_diagnostics(
    analyzer_outcomes: &[crate::mod_host::AnalyzerOutcome],
    entry_unit: &SourceUnit,
    entry_source: &str,
) -> Vec<SemanticDiagnostic> {
    let mut out = Vec::new();
    for outcome in analyzer_outcomes {
        for diagnostic in &outcome.diagnostics {
            out.push(analyzer_diagnostic_to_semantic(
                diagnostic,
                outcome.type_id.as_str(),
                entry_unit.logical_name.as_str(),
                entry_source,
            ));
        }
    }
    out
}

/// Map mod `Analyzer` quick-fixes into the LSP-facing [`SyntaxFix`] stream.
///
/// Each `AnalyzerFix` is bridged via [`analyzer_fix_to_syntax_fix`], which resolves the
/// linked diagnostic via `diagnostic_index` and tags the fix with
/// `source = "beskid:mod:<type_id>"`. Out-of-range fixes are dropped (fail-closed).
pub(super) fn collect_analyzer_fixes(analyzer_outcomes: &[crate::mod_host::AnalyzerOutcome]) -> Vec<SyntaxFix> {
    let mut out = Vec::new();
    for outcome in analyzer_outcomes {
        let source = format!("beskid:mod:{}", outcome.type_id);
        for fix in &outcome.fixes {
            if let Some(syntax_fix) = analyzer_fix_to_syntax_fix(fix, outcome, &source) {
                out.push(syntax_fix);
            }
        }
    }
    out
}

pub(super) fn semantic_facts_errors_to_diagnostics(
    error: SemanticFactsError,
    source_name: &str,
    source: &str,
) -> Vec<SemanticDiagnostic> {
    let mut ctx = RuleContext::new(source_name, source, AnalysisOptions::default());
    match error {
        SemanticFactsError::Type { errors, typed } => {
            for error in errors {
                types::emit_type_error(&mut ctx, error, Some(&typed));
            }
        }
        SemanticFactsError::Resolve(errors) => {
            for error in errors {
                resolve::emit_resolve_error(&mut ctx, error);
            }
        }
    }
    ctx.diagnostics
}

pub(super) fn typed_fingerprint(resolution: &crate::resolve::Resolution) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    resolution.items.len().hash(&mut hasher);
    resolution.tables.resolved_values.len().hash(&mut hasher);
    hasher.finish()
}

pub(super) fn typed_fingerprint_types(typed: &crate::types::TypeResult) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    typed.node_types.len().hash(&mut hasher);
    typed.lowering.cast_intents.len().hash(&mut hasher);
    hasher.finish()
}
