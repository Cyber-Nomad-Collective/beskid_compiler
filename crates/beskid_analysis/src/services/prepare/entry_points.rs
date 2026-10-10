//! Public prepare entry points: the executable spine and the fact-authority diagnostics spine.

use anyhow::Result;
use beskid_pipeline::PipelineObserver;

use crate::analysis::SemanticDiagnostic;
use crate::mod_host::SyntaxFix;
use crate::projects::ProgramAssembly;
use crate::syntax::Spanned;

use super::super::input::ResolvedInput;
use super::super::session::SessionFingerprint;
use super::*;

/// Single front-end spine: assemble → mod host → rewrite → semantic → composition → (optional) typed syntax.
pub fn prepare_compilation(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)"))?;

    let spine = run_prepare_spine(
        &resolved.source_path,
        &resolved.source,
        plan,
        resolved.prepared_workspace.as_ref(),
        resolved.assembly.as_ref(),
        &options,
        pipeline,
        false,
        true,
        None,
    )?;

    Ok(spine.prepared)
}

/// Diagnostics-only preparation lives behind [`prepare_compilation_with_fact_authority`]: the
/// query gate (`beskid_queries::prepare_compilation_diagnostics`, `_with_db`, `_isolated`) is the
/// one semantic authority of `beskid check`, `analyze`, and the language server, so no
/// authority-less diagnostics entry point exists here.
/// One positive semantic-fact finding from a generation-bound semantic owner: a registered
/// diagnostic kind located in one unit of the assembly the spine prepared. `unit` indexes
/// [`ProgramAssembly::units`]; a finding outside the entry unit renders against that unit's own
/// source, so `analyze` and the LSP show a dependency-unit error where it is, not in the entry.
#[derive(Debug, Clone)]
pub struct SemanticFactFinding {
    pub kind: crate::analysis::diagnostic_kinds::SemanticIssueKind,
    pub unit: usize,
    pub span: crate::syntax::SpanInfo,
}

/// Inversion seam for a generation-bound semantic owner. The callback consumes the final
/// rewritten entry and its actual assembly, not a second frontend tree, and returns every
/// finding its facts produce for the entry's reachable items (invalid `?` targets and the
/// reachability-scoped legality facts the lowering gate also evaluates).
pub trait SemanticFactAuthority {
    fn fact_findings(
        &mut self,
        assembly: &ProgramAssembly,
        program: &Spanned<crate::syntax::Program>,
    ) -> Result<Vec<SemanticFactFinding>>;
    fn mod_scope(&self) -> Option<&dyn crate::mod_host::ModSemanticScope> {
        None
    }
}
impl<F> SemanticFactAuthority for F
where
    F: FnMut(&ProgramAssembly, &Spanned<crate::syntax::Program>) -> Result<Vec<SemanticFactFinding>>,
{
    fn fact_findings(
        &mut self,
        assembly: &ProgramAssembly,
        program: &Spanned<crate::syntax::Program>,
    ) -> Result<Vec<SemanticFactFinding>> {
        self(assembly, program)
    }
}

pub fn prepare_compilation_with_fact_authority(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
    collect_diagnostics: bool,
    isolated: bool,
    authority: &mut dyn SemanticFactAuthority,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)> {
    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("prepare_compilation requires project context"))?;
    if isolated && resolved.assembly.is_none() {
        return Err(anyhow::anyhow!("isolated diagnostics require an owned program assembly"));
    }
    let output = run_prepare_spine(
        &resolved.source_path,
        &resolved.source,
        plan,
        resolved.prepared_workspace.as_ref(),
        resolved.assembly.as_ref(),
        &options,
        pipeline,
        collect_diagnostics,
        !isolated,
        Some(authority),
    )?;
    Ok((output.prepared, output.collected_diagnostics, output.collected_fixes))
}

pub(super) fn session_fingerprint_field(fingerprint: &SessionFingerprint) -> String {
    format!("{}:{}", fingerprint.entry_canonical.display(), fingerprint.lockfile_digest)
}
