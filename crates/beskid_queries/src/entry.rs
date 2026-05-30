//! Entry-spine queries: prepare, semantic gate, composition, typed HIR.

use std::sync::Arc;

use anyhow::Result;
use beskid_analysis::analysis::SemanticDiagnostic;
use beskid_analysis::services::{
    FrontEndOptions, FrontEndTypedResult, PrepareMode, PrepareOptions, PreparedCompilation,
    ResolvedInput,
};
use beskid_pipeline::{PipelineObserver, observe_phase, phases};

use crate::db::BeskidDatabase;
use crate::graph::program_assembly;
use crate::output::SharedFrontEnd;
use crate::stats::{emit_salsa_stats, record_query_miss};

/// Semantic gate diagnostics for an entry (query boundary marker).
pub fn semantic_gate_diagnostics(_db: &dyn crate::db::Db, fingerprint: &str) -> u64 {
    let _ = fingerprint;
    record_query_miss();
    0
}

/// Semantic snapshot fingerprint after gate (query boundary marker).
pub fn semantic_snapshot(_db: &dyn crate::db::Db, diagnostic_count: u64) -> u64 {
    record_query_miss();
    diagnostic_count
}

/// Full prepare spine via Salsa-backed assembly + existing analysis phases.
pub fn prepare_compilation_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<PreparedCompilation> {
    record_query_miss();
    let resolved = enrich_resolved_with_assembly(db, resolved, &options)?;
    db.set_file_text(resolved.source_path.clone(), resolved.source.clone());
    let result = beskid_analysis::services::prepare_compilation(&resolved, options, pipeline)?;
    emit_salsa_stats(pipeline);
    Ok(result)
}

pub fn prepare_compilation_diagnostics_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>)> {
    record_query_miss();
    let resolved = enrich_resolved_with_assembly(db, resolved, &options)?;
    db.set_file_text(resolved.source_path.clone(), resolved.source.clone());
    let result =
        beskid_analysis::services::prepare_compilation_diagnostics(&resolved, options, pipeline)?;
    let _ = semantic_snapshot(db, result.1.len() as u64);
    observe_phase(pipeline, phases::SEMANTIC_SNAPSHOT, || {});
    emit_salsa_stats(pipeline);
    Ok(result)
}

pub fn typed_entry_bundle(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let prepared = prepare_compilation_with_db(
        db,
        resolved,
        PrepareOptions {
            mode: PrepareMode::Executable,
            front_end: FrontEndOptions {
                with_semantic_diagnostics: false,
                ..Default::default()
            },
        },
        pipeline,
    )?;
    Ok(SharedFrontEnd(Arc::new(prepared.into_executable()?)))
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
    let mut assembly_options = beskid_analysis::projects::model::AssemblyOptions::default();
    assembly_options.discovery = options.front_end.assembly_discovery;
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
