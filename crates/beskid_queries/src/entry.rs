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
    let resolved = assemble_resolved_input_with_db(db, resolved, &options)?;
    let (result, _, _) = beskid_analysis::services::prepare_compilation_with_fact_authority(
        &resolved,
        options,
        pipeline,
        false,
        false,
        &mut |assembly, program| semantic_fact_findings(db, Some(&resolved), assembly, program),
    )?;
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
    let resolved = assemble_resolved_input_with_db(db, resolved, &options)?;
    let result = beskid_analysis::services::prepare_compilation_with_fact_authority(
        &resolved,
        options,
        pipeline,
        true,
        false,
        &mut |assembly, program| semantic_fact_findings(db, Some(&resolved), assembly, program),
    )?;
    if let Some(fp) = session_fingerprint(&resolved) {
        let _ = semantic_snapshot(db, &fingerprint_key(&fp));
    }
    observe_phase(pipeline, phases::SEMANTIC_SNAPSHOT, || {});
    touch_from_prepare(&resolved);
    emit_salsa_stats(pipeline);
    Ok(result)
}

/// Diagnose an owned editor job with the same generation-bound try query used by
/// codegen. This database is local to the job; no mutable process cache or second
/// syntax representation participates in its semantic decisions.
pub fn prepare_compilation_diagnostics_isolated(
    resolved: &ResolvedInput,
    options: PrepareOptions,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<(PreparedCompilation, Vec<SemanticDiagnostic>, Vec<beskid_analysis::SyntaxFix>)> {
    let mut db = BeskidDatabase::default();
    beskid_analysis::services::prepare_compilation_with_fact_authority(
        resolved,
        options,
        pipeline,
        true,
        true,
        &mut |assembly, program| semantic_fact_findings(&mut db, None, assembly, program),
    )
}

/// Collect the generation-bound semantic-fact findings of the entry's reachable items: every
/// try expression of the entry unit that has no try fact (E1222), and every finding of the
/// reachability-scoped legality gate (`check_items`, the same facts `lower_syntax_program`
/// evaluates) over the entry unit's function, method, and test items plus the direct-call closure
/// of each. A dependency-unit finding carries its own unit, so `analyze` and the LSP report it in
/// that unit's source. Items nothing in the entry reaches are not judged.
///
/// `planned` is the resolved input of a planned project entry on the shared
/// database. Its syntax is registered under the session that already owns the
/// assembly's units, or else under the plan-keyed registry session, so later
/// LSP/IDE fact queries over the same units find that owner instead of
/// colliding with a second, unregistered session. Only the isolated, job-local
/// database passes `None` and lets the assembly mint its own owner.
fn semantic_fact_findings(
    db: &mut BeskidDatabase,
    planned: Option<&ResolvedInput>,
    assembly: &beskid_analysis::projects::ProgramAssembly,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Result<Vec<beskid_analysis::services::SemanticFactFinding>> {
    use crate::{
        AstNodeKey, IndexedNodeKind, build_typed_program, project_session_for_syntax_assembly, try_expression_fact,
    };
    use beskid_analysis::analysis::SemanticIssueKind;
    use beskid_analysis::services::SemanticFactFinding;
    use std::sync::Arc;
    let mut units = assembly.units.as_ref().clone();
    units[assembly.entry_index].program = program.clone();
    let syntax = Arc::new(
        beskid_analysis::projects::ProgramAssembly::new(
            assembly.roots.clone(),
            Arc::new(units),
            assembly.entry_index,
            assembly.discovery,
            Arc::clone(&assembly.module_index),
            assembly.has_std_dependency,
            assembly.generation,
        )
        .with_trusted_corelib_service_paths(Arc::clone(&assembly.trusted_corelib_service_paths))
        .with_runtime_fixture(assembly.runtime_fixture.clone()),
    );
    let project = match planned.and_then(|resolved| Some((resolved.compile_plan.as_ref()?, resolved))) {
        Some((plan, resolved)) => crate::project_session_for_planned_syntax_assembly(
            db,
            &syntax,
            plan,
            &resolved.source_path,
            crate::typed_entry_bundle::lockfile_digest_for_plan(plan),
        )?,
        None => project_session_for_syntax_assembly(db, &syntax, "try-diagnostics", "source-authority")?,
    };
    let typed = build_typed_program(db, project, syntax.generation, Arc::clone(&syntax))?;
    let index = syntax.entry_syntax_index();
    let mut findings = Vec::new();
    for node in index.ids_of_kind(IndexedNodeKind::TryExpression) {
        let key = AstNodeKey { unit: typed.entry, generation: syntax.generation, node };
        if !matches!(try_expression_fact(db, key), Ok(Some(_))) {
            let span = index
                .node_at(program, node)
                .and_then(|node| node.span())
                .ok_or_else(|| anyhow::anyhow!("try diagnostic requires its exact source span"))?;
            findings.push(SemanticFactFinding {
                kind: SemanticIssueKind::TypeInvalidTryTarget,
                unit: syntax.entry_index,
                span,
            });
        }
    }
    let entry_root = AstNodeKey { unit: typed.entry, generation: syntax.generation, node: crate::AstNodeId(0) };
    let mut items = Vec::new();
    for kind in
        [IndexedNodeKind::FunctionDefinition, IndexedNodeKind::MethodDefinition, IndexedNodeKind::TestDefinition]
    {
        for node in index.ids_of_kind(kind) {
            let item = AstNodeKey { node, ..entry_root };
            // An item whose direct-call closure cannot be traced (an unresolved callee is itself a
            // legality finding) is still judged on its own body.
            let reachable = match crate::reachable_items(db, entry_root, item) {
                Ok(Some(reachable)) => reachable.to_vec(),
                Ok(None) | Err(_) => vec![item],
            };
            for key in reachable {
                if !items.contains(&key) {
                    items.push(key);
                }
            }
        }
    }
    if let Err(legality) = crate::check_items(db, &items) {
        let units = syntax
            .units
            .iter()
            .enumerate()
            .map(|(index, unit)| (crate::SourceUnitId::new(db, unit.path.clone()), index))
            .collect::<std::collections::HashMap<_, _>>();
        for finding in legality {
            let unit = *units
                .get(&finding.site.unit)
                .ok_or_else(|| anyhow::anyhow!("legality finding site is outside the prepared assembly"))?;
            let span = crate::node_span(db, finding.site)
                .ok()
                .flatten()
                .ok_or_else(|| anyhow::anyhow!("legality finding requires its exact source span"))?;
            findings.push(SemanticFactFinding { kind: finding.kind, unit, span });
        }
    }
    Ok(findings)
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
    let resolved = assemble_resolved_input_with_db(db, resolved, options)?;
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

/// Commit source inputs and derive the Salsa-backed syntax assembly.
///
/// Callers may release writer exclusivity after this phase and run downstream
/// analysis from the owned [`ResolvedInput`] assembly without holding Salsa.
pub fn assemble_resolved_input_with_db(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    options: &PrepareOptions,
) -> Result<ResolvedInput> {
    let enriched = if resolved.assembly.is_some() {
        clone_resolved(resolved)
    } else if let Some(plan) = resolved.compile_plan.as_ref() {
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
        resolved.with_assembly(assembly)
    } else {
        clone_resolved(resolved)
    };
    db.ensure_file_text(enriched.source_path.clone(), enriched.source.clone());
    Ok(enriched)
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
