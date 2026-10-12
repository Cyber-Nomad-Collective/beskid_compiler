//! Compile-plan lookup for analysis inputs and the pre-parse reserved-keyword diagnostic.
//!
//! Every diagnostics path, project-backed or standalone, is judged by the query gate through
//! `beskid_queries::prepare_compilation_diagnostics` and its `_with_db` / `_isolated` variants;
//! this crate offers no authority-less analysis path.

use std::path::Path;

use crate::analysis::SemanticDiagnostic;
use crate::compilation_context::ProjectSessionHandle;
use crate::projects::CompilePlan;

/// The reserved-keyword diagnostic of a source, reported before parsing: `async` and `await`
/// are reserved and have no syntax, so every analysis path reports them with their own codes
/// instead of a parse error.
pub fn reserved_keyword_diagnostic(path: &Path, source: &str) -> Option<SemanticDiagnostic> {
    use crate::analysis::diagnostic_kinds::SemanticIssueKind;
    use crate::analysis::diagnostics::make_diagnostic;
    use crate::parsing::reserved_keywords::ReservedKeyword;

    let (span, keyword) = crate::parsing::reserved_keywords::find_reserved_keyword(source)?;
    let kind = match keyword {
        ReservedKeyword::Async => SemanticIssueKind::AsyncKeywordReserved,
        ReservedKeyword::Await => SemanticIssueKind::AwaitKeywordReserved,
    };
    Some(make_diagnostic(
        &path.display().to_string(),
        source,
        span,
        kind.message(),
        kind.label(),
        None,
        Some(kind.code().to_string()),
        crate::analysis::Severity::Error,
    ))
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
