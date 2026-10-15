//! Public prepare entry point: the spine driven by a generation-bound semantic fact authority.
//!
//! Every preparation, executable or diagnostics-only, runs behind
//! [`prepare_compilation_with_fact_authority`]. The query gate
//! (`beskid_queries::{prepare_compilation, prepare_compilation_diagnostics}`, `_with_db`,
//! `_isolated`) is the one semantic authority of `beskid build`, `run`, `test`, `check`,
//! `analyze`, and the language server, so no authority-less entry point exists here.

use anyhow::Result;
use beskid_pipeline::PipelineObserver;

use crate::analysis::SemanticDiagnostic;
use crate::mod_host::SyntaxFix;
use crate::projects::ProgramAssembly;
use crate::syntax::Spanned;

use super::super::input::ResolvedInput;
use super::super::session::SessionFingerprint;
use super::*;

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
/// finding its facts produce for every own unit (typing and legality obligations, invalid `?`
/// targets, and the reachability-scoped legality facts the lowering gate also evaluates).
pub trait SemanticFactAuthority {
    /// `pipeline` observes the gate's reported sub-phases (`semantic.contracts` around contract
    /// conformance); the authority forwards it, never records phases of its own.
    fn fact_findings(
        &mut self,
        assembly: &ProgramAssembly,
        program: &Spanned<crate::syntax::Program>,
        pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
    ) -> Result<Vec<SemanticFactFinding>>;
    fn mod_scope(&self) -> Option<&dyn crate::mod_host::ModSemanticScope> {
        None
    }
}
impl<F> SemanticFactAuthority for F
where
    F: FnMut(
        &ProgramAssembly,
        &Spanned<crate::syntax::Program>,
        Option<&dyn beskid_pipeline::PipelineObserver>,
    ) -> Result<Vec<SemanticFactFinding>>,
{
    fn fact_findings(
        &mut self,
        assembly: &ProgramAssembly,
        program: &Spanned<crate::syntax::Program>,
        pipeline: Option<&dyn beskid_pipeline::PipelineObserver>,
    ) -> Result<Vec<SemanticFactFinding>> {
        self(assembly, program, pipeline)
    }
}

/// Run the prepare spine: assemble, Mod host, rewrite, structural rules, composition, query gate,
/// executable front end. With `collect_diagnostics` every diagnostic is returned; otherwise an
/// error diagnostic fails preparation.
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
        authority,
    )?;
    Ok((output.prepared, output.collected_diagnostics, output.collected_fixes))
}

pub(super) fn session_fingerprint_field(fingerprint: &SessionFingerprint) -> String {
    format!("{}:{}", fingerprint.entry_canonical.display(), fingerprint.lockfile_digest)
}
