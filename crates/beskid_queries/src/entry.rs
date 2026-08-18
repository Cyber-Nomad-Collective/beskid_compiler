//! Entry-spine queries: prepare, semantic gate, composition, typed HIR.

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{
    FrontEndOptions, PrepareOptions, PreparedCompilation, ResolvedInput, SemanticSnapshot, SessionFingerprint,
    cached_semantic_snapshot, invalidate_entry_sessions_for_project,
};
use beskid_pipeline::{PipelineObserver, observe_phase, phases};

use crate::db::BeskidDatabase;
use crate::graph::program_assembly;
use crate::output::{SharedFrontEnd, SharedResolution};
use crate::stats::{emit_salsa_stats, record_revision_bump, trace_query};

pub fn session_fingerprint(resolved: &ResolvedInput) -> Option<SessionFingerprint> {
    let plan = resolved.compile_plan.as_ref()?;
    Some(SessionFingerprint::for_entry(plan, &resolved.source_path))
}

/// Semantic gate diagnostics fingerprint for an entry (reads entry session registry).
pub fn semantic_gate_diagnostics(_db: &dyn crate::db::Db, fingerprint: &str) -> u64 {
    let fp = decode_fingerprint_key(fingerprint);
    if let Some(snapshot) = cached_semantic_snapshot(&fp) {
        trace_query("semantic_gate_diagnostics", true);
        return snapshot.diagnostic_fingerprint;
    }
    trace_query("semantic_gate_diagnostics", false);
    0
}

/// Semantic snapshot diagnostic count after gate (reads entry session registry).
pub fn semantic_snapshot(_db: &dyn crate::db::Db, fingerprint: &str) -> u64 {
    let fp = decode_fingerprint_key(fingerprint);
    if let Some(snapshot) = cached_semantic_snapshot(&fp) {
        trace_query("semantic_snapshot", true);
        return snapshot.diagnostic_count as u64;
    }
    trace_query("semantic_snapshot", false);
    0
}

/// Registry lookup for tooling/tests.
pub fn cached_semantic_snapshot_for_key(fingerprint: &str) -> Option<SemanticSnapshot> {
    cached_semantic_snapshot(&decode_fingerprint_key(fingerprint))
}

fn decode_fingerprint_key(key: &str) -> SessionFingerprint {
    let mut parts = key.splitn(3, '\0');
    let project_root = parts.next().map(std::path::PathBuf::from).unwrap_or_default();
    let entry_canonical = parts.next().map(std::path::PathBuf::from).unwrap_or_default();
    let lockfile_digest = parts.next().and_then(|value| value.parse().ok()).unwrap_or(0);
    SessionFingerprint { project_root, entry_canonical, lockfile_digest }
}

pub fn fingerprint_key(fingerprint: &SessionFingerprint) -> String {
    format!(
        "{}\0{}\0{}",
        fingerprint.project_root.display(),
        fingerprint.entry_canonical.display(),
        fingerprint.lockfile_digest
    )
}

fn touch_from_prepare(resolved: &ResolvedInput) {
    if session_fingerprint(resolved).is_some() {
        record_revision_bump();
    }
}

/// Full prepare spine via Salsa-backed assembly + existing analysis phases.
pub fn prepare_compilation_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    trace_query("prepare_compilation_with_db", false);
    let resolved = enrich_resolved_with_assembly(db, resolved, &options)?;
    db.ensure_file_text(resolved.source_path.clone(), resolved.source.clone());
    let result = beskid_analysis::services::prepare_compilation(&resolved, options, pipeline)?;
    touch_from_prepare(&resolved);
    emit_salsa_stats(pipeline);
    Ok(result)
}

pub fn prepare_compilation_diagnostics_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<beskid_analysis::SyntaxFix>)> {
    trace_query("prepare_compilation_diagnostics_with_db", false);
    let resolved = enrich_resolved_with_assembly(db, resolved, &options)?;
    db.ensure_file_text(resolved.source_path.clone(), resolved.source.clone());
    let result = beskid_analysis::services::prepare_compilation_diagnostics(&resolved, options, pipeline)?;
    if let Some(fp) = session_fingerprint(&resolved) {
        let _ = semantic_snapshot(db, &fingerprint_key(&fp));
    }
    observe_phase(pipeline, phases::SEMANTIC_SNAPSHOT, || {});
    touch_from_prepare(&resolved);
    emit_salsa_stats(pipeline);
    Ok(result)
}

pub fn typed_entry_bundle(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let options = PrepareOptions {
        front_end: FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        ..Default::default()
    };
    crate::typed_entry_bundle::typed_entry_bundle_with_db(db, resolved, &options, pipeline)
}

/// Entry resolution only (assembly + module index resolve, no typecheck).
pub fn entry_resolution_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: &PrepareOptions,
) -> Result<SharedResolution> {
    trace_query("entry_resolution_with_db", false);
    let resolved = enrich_resolved_with_assembly(db, resolved, options)?;
    db.ensure_file_text(resolved.source_path.clone(), resolved.source.clone());
    let assembly =
        resolved.assembly.as_ref().ok_or_else(|| anyhow::anyhow!("entry resolution requires assembled program"))?;
    let resolution = beskid_analysis::services::resolve_entry(
        &assembly.entry_unit().program,
        assembly,
        Some(resolved.source_path.as_path()),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(SharedResolution::from_resolution(resolution))
}

fn enrich_resolved_with_assembly(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: &PrepareOptions,
) -> Result<ResolvedInput> {
    if resolved.assembly.is_some() {
        return Ok(clone_resolved(resolved));
    }
    let Some(plan) = resolved.compile_plan.as_ref() else {
        return Ok(clone_resolved(resolved));
    };
    let assembly_options =
        beskid_analysis::projects::assembly_options_for_prepare(plan, options.front_end.assembly_discovery);
    let assembly = program_assembly(
        db,
        plan,
        resolved.prepared_workspace.as_ref(),
        &resolved.source_path,
        Some(&resolved.source),
        &assembly_options,
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    Ok(resolved.with_assembly(assembly))
}

fn clone_resolved(resolved: &ResolvedInput) -> ResolvedInput {
    ResolvedInput {
        source_path: resolved.source_path.clone(),
        source: resolved.source.clone(),
        compile_plan: resolved.compile_plan.clone(),
        prepared_workspace: resolved.prepared_workspace.clone(),
        workspace_summary: resolved.workspace_summary.clone(),
        assembly: resolved.assembly.clone(),
    }
}

/// Clear entry-session registry slices for a project root (LSP / workspace invalidation).
pub fn invalidate_entry_sessions(project_root: &std::path::Path) {
    invalidate_entry_sessions_for_project(project_root);
}
