//! Public prepare entry points: full, diagnostics-only, isolated, and try-authority variants.

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

/// Like [`prepare_compilation`], collecting diagnostics from semantic, composition, and lower
/// instead of failing on errors. Also collects mod-origin quick-fixes from `Analyzer`
/// outcomes so LSP code actions can surface `QUICKFIX` actions keyed by
/// `(source = "beskid:mod:<type_id>", code)`.
pub fn prepare_compilation_diagnostics(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)> {
    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)"))?;

    let mut diagnostics = Vec::new();
    let mut fixes = Vec::new();
    let spine = run_prepare_spine(
        &resolved.source_path,
        &resolved.source,
        plan,
        resolved.prepared_workspace.as_ref(),
        resolved.assembly.as_ref(),
        &options,
        pipeline,
        true,
        true,
        None,
    )?;
    diagnostics.extend(spine.collected_diagnostics);
    fixes.extend(spine.collected_fixes);
    Ok((spine.prepared, diagnostics, fixes))
}

/// Collect diagnostics from an owned assembly without consulting or updating
/// the process-wide entry-session cache.
///
/// This is intended for overlapping editor jobs whose version identity is
/// enforced by the caller. It requires `resolved.assembly` so no global
/// assembly/session authority can substitute a different generation.
pub fn prepare_compilation_diagnostics_isolated(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<SyntaxFix>)> {
    let plan = resolved
        .compile_plan
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("prepare_compilation requires a CompilePlan (project context)"))?;
    if resolved.assembly.is_none() {
        return Err(anyhow::anyhow!("isolated diagnostics require an owned program assembly"));
    }
    let spine = run_prepare_spine(
        &resolved.source_path,
        &resolved.source,
        plan,
        resolved.prepared_workspace.as_ref(),
        resolved.assembly.as_ref(),
        &options,
        pipeline,
        true,
        false,
        None,
    )?;
    Ok((spine.prepared, spine.collected_diagnostics, spine.collected_fixes))
}

/// Inversion seam for a generation-bound semantic owner. The callback consumes
/// the final rewritten entry and its actual assembly, not a second frontend tree.
pub type TryDiagnosticAuthority<'a> =
    dyn FnMut(&ProgramAssembly, &Spanned<crate::syntax::Program>) -> Result<Vec<crate::syntax::SpanInfo>> + 'a;

pub fn prepare_compilation_with_try_authority(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
    collect_diagnostics: bool,
    isolated: bool,
    authority: &mut TryDiagnosticAuthority<'_>,
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
