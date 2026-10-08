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
    let mut authority = crate::mod_semantic_scope::QueryModSemanticScope::new(
        db,
        Some(&resolved),
        resolved.assembly.as_ref().ok_or_else(|| anyhow::anyhow!("prepared Mod authority requires assembly"))?.clone(),
    );
    let (result, _, _) = beskid_analysis::services::prepare_compilation_with_fact_authority(
        &resolved,
        options,
        pipeline,
        false,
        false,
        &mut authority,
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
    let mut authority = crate::mod_semantic_scope::QueryModSemanticScope::new(
        db,
        Some(&resolved),
        resolved.assembly.as_ref().ok_or_else(|| anyhow::anyhow!("prepared Mod authority requires assembly"))?.clone(),
    );
    let result = beskid_analysis::services::prepare_compilation_with_fact_authority(
        &resolved,
        options,
        pipeline,
        true,
        false,
        &mut authority,
    )?;
    drop(authority);
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
    let mut authority = crate::mod_semantic_scope::QueryModSemanticScope::new(
        &mut db,
        None,
        resolved.assembly.as_ref().ok_or_else(|| anyhow::anyhow!("isolated Mod authority requires assembly"))?.clone(),
    );
    beskid_analysis::services::prepare_compilation_with_fact_authority(
        resolved,
        options,
        pipeline,
        true,
        true,
        &mut authority,
    )
}

/// The query-backed semantic diagnostics gate of `beskid check` over every root unit.
///
/// The roots are the entry unit, or every own unit of an entry-less library. One `TypedProgram`
/// is built for the whole assembly with the same canonical Corelib service capability codegen
/// installs (`canonical_corelib_syscall_service_capability`, or the runtime fixture authority),
/// so a declaration the lowering gate accepts is never "unknown" here. Then every item of every
/// root unit is judged -- each function, method (including those inside `type` and `impl`
/// blocks), and test the unit's syntax index holds, whatever reaches it -- on three obligation
/// sets:
///
/// * every try expression of a root unit that has no try fact (E1222);
/// * the legality gate (`check_items`, the same facts `lower_syntax_program` evaluates) over the
///   root items plus the direct-call closure of each, so a dependency-unit error an own item
///   reaches is reported in that unit's source;
/// * the typing obligations (`check_typing_obligations`: E1206, E1207, E1208) over the root
///   items themselves. Dependency bodies are judged by their own project;
/// * the remaining legacy `TypeChecker` classes (`check_gate_obligations`: operators, numeric
///   literals, spawn, iterators, events, expression shapes, member and call targets, duplicate
///   locals, generic bounds) over the root items, the unit-level obligations
///   (`check_unit_obligations`: event capacity, contract conformance, `This` and associated
///   types) once per root unit, and the extern profile (`extern_profile_findings`: T0901-T0904,
///   Glue bindings for the manifest's `glue` libraries) once per root unit.
///
/// Root reachability is never applied: a library item nothing calls is still judged.
///
/// `planned` is the resolved input of a planned project entry on the shared
/// database. Its syntax is registered under the session that already owns the
/// assembly's units, or else under the plan-keyed registry session, so later
/// LSP/IDE fact queries over the same units find that owner instead of
/// colliding with a second, unregistered session. Only the isolated, job-local
/// database passes `None` and lets the assembly mint its own owner.
pub fn semantic_diagnostics_for_roots(
    db: &mut BeskidDatabase,
    planned: Option<&ResolvedInput>,
    assembly: &beskid_analysis::projects::ProgramAssembly,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
) -> Result<Vec<beskid_analysis::services::SemanticFactFinding>> {
    use crate::{
        AstNodeKey, IndexedNodeKind, build_runtime_fixture_typed_program,
        build_typed_program_with_corelib_syscall_services, project_session_for_syntax_assembly, try_expression_fact,
    };
    use beskid_abi::abi_v5::AbiManifestV5;
    use beskid_analysis::analysis::SemanticIssueKind;
    use beskid_analysis::services::SemanticFactFinding;
    use std::sync::Arc;
    let mut units = assembly.units.as_ref().clone();
    units[assembly.entry_index].program = program.clone();
    let mut syntax = beskid_analysis::projects::ProgramAssembly::new(
        assembly.roots.clone(),
        Arc::new(units),
        assembly.entry_index,
        assembly.discovery,
        Arc::clone(&assembly.module_index),
        assembly.has_core_dependency,
        assembly.generation,
    )
    .with_recovery_policy(assembly.recovery_policy)
    .with_trusted_corelib_service_paths(Arc::clone(&assembly.trusted_corelib_service_paths))
    .with_glue_libraries(Arc::clone(&assembly.glue_libraries))
    .with_runtime_fixture(assembly.runtime_fixture.clone())
    .with_root_set(assembly.root_set.clone());
    syntax.verified_package_identities = assembly.package_identities().clone();
    let syntax = Arc::new(syntax);
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
    // Preparation runs before the codegen target is selected. Source authority is independent of
    // target metadata, but must be installed with the exact constructors
    // `beskid_codegen::prepared_syntax` uses (`canonical_corelib_syscall_service_capability` and
    // `build_typed_program_with_corelib_syscall_services`, or the runtime fixture authority);
    // otherwise the legality gate calls compiler-owned services and intrinsics "unknown".
    let authority_target = beskid_abi::runtime_kit::host_runtime_target()
        .map_err(|error| anyhow::anyhow!("host ABI-v5 target unavailable for source authority: {error}"))?;
    let manifest = AbiManifestV5::canonical_runtime(authority_target);
    let typed = if syntax.runtime_fixture.is_some() {
        build_runtime_fixture_typed_program(db, project, syntax.generation, Arc::clone(&syntax), &manifest)?
    } else {
        let capability = beskid_abi::runtime_source::canonical_corelib_syscall_service_capability(&manifest)
            .map_err(|error| anyhow::anyhow!("canonical Corelib service authority unavailable: {error:?}"))?;
        build_typed_program_with_corelib_syscall_services(
            db,
            project,
            syntax.generation,
            Arc::clone(&syntax),
            capability,
        )?
    };
    // Every root unit is judged: the entry, plus each own unit of an entry-less library.
    let root_indices = syntax
        .root_unit_indices()
        .ok_or_else(|| anyhow::anyhow!("assembly root set names a source unit outside the prepared assembly"))?;
    let mut findings = Vec::new();
    // Every item of every root unit, in root order: the typing-obligation subjects.
    let mut root_items = Vec::new();
    // The root unit keys (`AstNodeId(0)`), in root order: the unit-level obligation subjects.
    let mut root_units = Vec::new();
    // Extern profile findings, evaluated per root unit with the assembly facts the database does
    // not hold (manifest Glue libraries, canonical Dynamic unit exemption).
    let mut extern_findings = Vec::new();
    // The root items plus the direct-call closure of each: the legality-gate subjects.
    let mut items = Vec::new();
    let mut seen_items = std::collections::HashSet::new();
    for root_index in root_indices {
        let root_unit = if root_index == syntax.entry_index {
            typed.entry
        } else {
            crate::SourceUnitId::new(db, syntax.units[root_index].path.clone())
        };
        let root_program = &syntax.units[root_index].program;
        let index = &syntax.syntax_indexes[root_index];
        for node in index.ids_of_kind(IndexedNodeKind::TryExpression) {
            let key = AstNodeKey { unit: root_unit, generation: syntax.generation, node };
            if !matches!(try_expression_fact(db, key), Ok(Some(_))) {
                let span = index
                    .node_at(root_program, node)
                    .and_then(|node| node.span())
                    .ok_or_else(|| anyhow::anyhow!("try diagnostic requires its exact source span"))?;
                findings.push(SemanticFactFinding {
                    kind: SemanticIssueKind::TypeInvalidTryTarget,
                    unit: root_index,
                    span,
                });
            }
        }
        let root = AstNodeKey { unit: root_unit, generation: syntax.generation, node: crate::AstNodeId(0) };
        root_units.push(root);
        extern_findings.extend(crate::extern_profile_findings(
            db,
            root,
            &syntax.glue_libraries,
            syntax.is_canonical_public_dynamic_unit(&syntax.units[root_index]),
        ));
        for kind in
            [IndexedNodeKind::FunctionDefinition, IndexedNodeKind::MethodDefinition, IndexedNodeKind::TestDefinition]
        {
            for node in index.ids_of_kind(kind) {
                let item = AstNodeKey { node, ..root };
                root_items.push(item);
                // An item whose direct-call closure cannot be traced (an unresolved callee is
                // itself a legality finding) is still judged on its own body.
                let reachable = match crate::reachable_items(db, root, item) {
                    Ok(Some(reachable)) => reachable.to_vec(),
                    Ok(None) | Err(_) => vec![item],
                };
                for key in reachable {
                    if seen_items.insert(key) {
                        items.push(key);
                    }
                }
            }
        }
    }
    let mut semantic_findings = crate::check_items(db, &items).err().unwrap_or_default();
    semantic_findings.extend(crate::check_typing_obligations(db, &root_items));
    semantic_findings.extend(crate::check_gate_obligations(db, &root_items));
    semantic_findings.extend(crate::check_unit_obligations(db, &root_units));
    semantic_findings.extend(extern_findings);
    if !semantic_findings.is_empty() {
        let units = syntax
            .units
            .iter()
            .enumerate()
            .map(|(index, unit)| (crate::SourceUnitId::new(db, unit.path.clone()), index))
            .collect::<std::collections::HashMap<_, _>>();
        for finding in semantic_findings {
            let unit = *units
                .get(&finding.site.unit)
                .ok_or_else(|| anyhow::anyhow!("semantic finding site is outside the prepared assembly"))?;
            let span = crate::node_span(db, finding.site)
                .ok()
                .flatten()
                .ok_or_else(|| anyhow::anyhow!("semantic finding requires its exact source span"))?;
            findings.push(SemanticFactFinding { kind: finding.kind, unit, span });
        }
    }
    Ok(findings)
}

#[cfg(test)]
#[path = "entry/parity_tests.rs"]
mod parity_tests;

pub fn typed_entry_bundle(
    db: &mut BeskidDatabase,
    resolved: &ResolvedInput,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<SharedFrontEnd> {
    let options = PrepareOptions {
        mod_invoker: None,
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
        let mut assembly_options =
            beskid_analysis::projects::assembly_options_for_prepare(plan, options.front_end.assembly_discovery);
        assembly_options.recovery_policy = options.front_end.assembly_recovery;
        assembly_options.native_mod_adapter_sources = options.native_mod_adapter_sources;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::{parse_program_with_source_name, synthetic_compile_plan_for_source};
    use beskid_analysis::syntax::SyntaxGenerationId;
    use beskid_analysis::syntax_query::SyntaxIndex;

    /// The exact embedded Slice source has service authority. The prepare-time legality gate
    /// must recognize `__panic` in its `Copy` method.
    #[test]
    fn canonical_corelib_service_survives_prepare_legality_gate() {
        let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
            .into_iter()
            .find(|source| source.logical_path == "Core/Bytes/Slice.bd")
            .expect("embedded canonical Slice source");
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&source.logical_path)
            .expect("canonical Slice identity");
        let path = identity.canonical_path;
        let generation = SyntaxGenerationId(413);
        let unit = SourceUnit {
            logical_name: source.logical_path,
            origin_path: path.clone(),
            program: parse_program_with_source_name(path.to_str().unwrap(), &source.source).unwrap(),
            path: path.clone(),
            source: source.source.clone(),
        };
        let plan = synthetic_compile_plan_for_source(&path);
        let roots = EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: path.parent().unwrap().into() },
            dependencies: vec![],
        };
        let indexes = vec![SyntaxIndex::from_program(&unit.program, generation)];
        let index = Arc::new(ModuleIndex::build(std::slice::from_ref(&unit), &indexes, &roots, &plan));
        let program = unit.program.clone();
        let assembly = ProgramAssembly::new(
            roots,
            Arc::new(vec![unit]),
            0,
            AssemblyDiscovery::ImportClosure,
            index,
            false,
            generation,
        );
        let mut db = BeskidDatabase::default();
        let findings = semantic_diagnostics_for_roots(&mut db, None, &assembly, &program)
            .expect("canonical Corelib semantic fact findings");
        assert!(
            !findings.iter().any(|finding| {
                matches!(&finding.kind, beskid_analysis::analysis::SemanticIssueKind::ResolveUnknownValue { name }
                    if name == "__panic")
            }),
            "canonical Corelib service rejected by legality gate: {findings:?}"
        );
    }

    /// The exact runtime fixture carries intrinsic authority into dependency units reached by
    /// its tests; `NativePointer` is supplied by the compiler, not declared as a source function.
    #[test]
    fn runtime_fixture_intrinsic_survives_prepare_legality_gate() {
        use beskid_abi::runtime_source::runtime_fixture_project_root;
        use beskid_analysis::projects::{
            assemble_program_with_materializer, assembly_options_for_plan, build_compile_plan, plan_entry_path,
        };

        let manifest = runtime_fixture_project_root().join("runtime_semantics.bproj");
        let plan = build_compile_plan(&manifest, Some("LifecycleTests")).expect("runtime fixture plan");
        let entry = plan_entry_path(&plan, &plan.source_root);
        let source = std::fs::read_to_string(&entry).expect("runtime fixture source");
        let assembly = assemble_program_with_materializer(
            &plan,
            None,
            &entry,
            Some(&source),
            &assembly_options_for_plan(&plan),
            None,
            None,
        )
        .expect("exact runtime fixture assembly");
        let program = assembly.entry_unit().program.clone();
        let mut db = BeskidDatabase::default();
        let findings = semantic_diagnostics_for_roots(&mut db, None, &assembly, &program)
            .expect("runtime fixture semantic fact findings");
        assert!(
            !findings.iter().any(|finding| {
                matches!(&finding.kind, beskid_analysis::analysis::SemanticIssueKind::ResolveUnknownValue { name }
                    if name == "NativePointer" || name == "CurrentThreadState")
            }),
            "canonical runtime intrinsic rejected by legality gate: {findings:?}"
        );
    }

    /// The CLI `check` spine (Salsa-backed assembly plus the fact authority) judges every own unit
    /// of an entry-less library, so a type error in its second unit by path is reported there.
    #[test]
    fn entry_less_lib_reports_type_error_in_second_own_unit_through_query_spine() {
        use beskid_analysis::analysis::diagnostics::Severity;
        use beskid_analysis::projects::{AssemblyRootSet, CompilePlan, Target, TargetKind, plan_entry_path};

        let root = std::env::temp_dir().join(format!("beskid_query_entry_less_lib_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source_root = root.join("src");
        std::fs::create_dir_all(&source_root).expect("host source root");
        std::fs::write(source_root.join("Alpha.bd"), "pub i64 Alpha() { return 1_i64; }").expect("Alpha");
        std::fs::write(source_root.join("Beta.bd"), "pub i64 Broken() { return true; }").expect("Beta");
        let plan = CompilePlan {
            source_root: source_root.clone(),
            project_root: root.clone(),
            manifest_path: root.join("fixture.bproj"),
            project_name: "fixture".to_string(),
            target: Target { name: "Library".to_string(), kind: TargetKind::Lib, entry: None },
            dependency_projects: Vec::new(),
            unresolved_dependencies: Vec::new(),
            has_core_dependency: false,
        };
        let entry_path = plan_entry_path(&plan, &source_root);
        let resolved =
            beskid_analysis::services::resolved_input_from_plan(entry_path, String::new(), plan, None, None);
        let mut db = BeskidDatabase::default();
        let (prepared, diagnostics, _) =
            prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::default(), None)
                .expect("prepare diagnostics");
        assert!(matches!(prepared.assembly.root_set, AssemblyRootSet::OwnUnits(ref paths) if paths.len() == 2));
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == Severity::Error && diagnostic.src.name().ends_with("Beta.bd")
            }),
            "type error in the second own unit must be reported: {diagnostics:?}"
        );
        assert!(
            !diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == Severity::Error && diagnostic.src.name().ends_with("Alpha.bd")
            }),
            "healthy own unit stays clean: {diagnostics:?}"
        );
        beskid_analysis::services::invalidate_entry_sessions_for_project(&root);
        let _ = std::fs::remove_dir_all(&root);
    }
}
