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
/// A genuine invalid `?` target is independently detected twice on the `try_authority` path: once
/// by the authority callback (driven by the typed `try_expression_fact`, emitted just above) and
/// again by `resolve_and_type_program_with_assembly`'s own full type check a few lines later,
/// which runs the same `is_result_shaped_enum` judgment through `type_try_expression` and,
/// on failure, surfaces `TypeError::InvalidTryTarget` via `semantic_facts_errors_to_diagnostics`.
/// Both sites are correct in isolation; only the caller-facing duplicate needs suppressing, so
/// this dedups by exact (span, code, message) rather than special-casing E1222 alone.
pub(super) fn dedupe_diagnostics(diagnostics: Vec<SemanticDiagnostic>) -> Vec<SemanticDiagnostic> {
    let mut seen: std::collections::HashSet<(usize, usize, Option<String>, String)> = std::collections::HashSet::new();
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            let key =
                (diagnostic.span.offset(), diagnostic.span.len(), diagnostic.code.clone(), diagnostic.message.clone());
            seen.insert(key)
        })
        .collect()
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
