//! Project-less analysis: parse a standalone source, run the structural rules, and report
//! composition diagnostics.
//!
//! Project-backed diagnostics (`beskid check`, `analyze`, the language server) are judged by the
//! query gate through `beskid_queries::prepare_compilation_diagnostics` and its `_with_db` /
//! `_isolated` variants; this crate offers no authority-less project diagnostics path.

use std::path::Path;

use anyhow::Result;

use crate::analysis::SemanticDiagnostic;
use crate::compilation_context::ProjectSessionHandle;
use crate::projects::CompilePlan;

use super::composition::composition_diagnostics_for_program;
use super::parse::parse_program_with_source_name;

pub fn analyze_program(path: &Path, source: &str) -> Result<Vec<SemanticDiagnostic>> {
    analyze_program_with_options(path, source, crate::AnalysisOptions::default())
}

/// Structural rule and composition diagnostics of one standalone source outside any project.
pub fn analyze_program_with_options(
    path: &Path,
    source: &str,
    _options: crate::AnalysisOptions,
) -> Result<Vec<SemanticDiagnostic>> {
    if let Some((span, keyword)) = crate::parsing::reserved_keywords::find_reserved_keyword(source) {
        use crate::analysis::diagnostic_kinds::SemanticIssueKind;
        use crate::analysis::diagnostics::make_diagnostic;
        use crate::parsing::reserved_keywords::ReservedKeyword;

        let (code, message) = match keyword {
            ReservedKeyword::Async => {
                (SemanticIssueKind::AsyncKeywordReserved.code(), SemanticIssueKind::AsyncKeywordReserved.message())
            }
            ReservedKeyword::Await => {
                (SemanticIssueKind::AwaitKeywordReserved.code(), SemanticIssueKind::AwaitKeywordReserved.message())
            }
        };
        let label = match keyword {
            ReservedKeyword::Async => SemanticIssueKind::AsyncKeywordReserved.label(),
            ReservedKeyword::Await => SemanticIssueKind::AwaitKeywordReserved.label(),
        };
        return Ok(vec![make_diagnostic(
            &path.display().to_string(),
            source,
            span,
            message,
            label,
            None,
            Some(code.to_string()),
            crate::analysis::Severity::Error,
        )]);
    }

    let source_name = path.display().to_string();
    let program = parse_program_with_source_name(&source_name, source)?;
    let mut diagnostics = super::semantic::semantic_rule_diagnostics_for_program(
        &program.node,
        source_name.clone(),
        source,
        crate::AnalysisOptions::default(),
    );
    diagnostics.extend(composition_diagnostics_for_program(&program, None, &source_name, source)?);
    Ok(diagnostics)
}

pub fn compile_plan_for_input_path(path: &Path) -> Option<CompilePlan> {
    if !path.is_file() {
        return None;
    }
    ProjectSessionHandle::try_for_analysis_path(path, None).and_then(|c| c.compile_plan)
}

pub fn compile_plan_for_input_path_with_member(path: &Path, workspace_member: Option<&str>) -> Option<CompilePlan> {
    if !path.is_file() {
        return None;
    }
    ProjectSessionHandle::try_for_analysis_path(path, workspace_member).and_then(|c| c.compile_plan)
}
