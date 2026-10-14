//! Real runtime fixture assembly; no surrogate runtime implementation is used.
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::runtime_fixture_project_root;
use beskid_analysis::projects::{
    ProgramAssembly, assemble_program_with_materializer, assembly_options_for_plan, build_compile_plan, plan_entry_path,
};
use beskid_queries::{
    BeskidDatabase, build_runtime_fixture_typed_program, project_session_for_syntax_assembly,
    semantic_diagnostics_for_roots,
};
use std::sync::Arc;

fn fixture(target: &str) -> ProgramAssembly {
    let manifest = runtime_fixture_project_root().join("runtime_semantics.bproj");
    let plan = build_compile_plan(&manifest, Some(target)).expect("real fixture plan");
    assert!(!plan.has_core_dependency, "installed Core must not contaminate runtime module identities");
    let entry = plan_entry_path(&plan, &plan.source_root);
    let source = std::fs::read_to_string(&entry).expect("fixture source");
    assemble_program_with_materializer(
        &plan,
        None,
        &entry,
        Some(&source),
        &assembly_options_for_plan(&plan),
        None,
        None,
    )
    .expect("exact runtime and real Corelib assembly")
}

fn manifest() -> AbiManifestV5 {
    AbiManifestV5::canonical_runtime(
        TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .expect("supported Linux target"),
    )
}

#[test]
fn real_runtime_fixtures_resolve_actual_module_signatures_and_constants() {
    for name in ["LifecycleTests", "ExternalWorkTests", "NetworkNativeTests"] {
        let assembly = fixture(name);
        let mut db = BeskidDatabase::default();
        let findings = semantic_diagnostics_for_roots(&mut db, None, &assembly, &assembly.entry_unit().program, None)
            .unwrap_or_else(|error| panic!("{name}: the query gate must judge the fixture: {error:#}"));
        let errors = findings
            .iter()
            .filter(|finding| finding.kind.severity() == beskid_analysis::Severity::Error)
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}

/// Regression: a `Project.lock` plus `obj/beskid/{deps,root}` snapshot left beside the fixture by
/// an earlier `beskid test` run (or copied with a builder slot's state) was replayed as the
/// fixture's source roots, so the assembly carried a second copy of every canonical Foundation
/// unit and `Core.Results` resolved ambiguously. The fixture assembly reads only the
/// plan-declared checkout roots, whatever local build state sits beside it.
#[test]
fn fixture_assembly_never_replays_local_materialized_snapshots() {
    use beskid_analysis::projects::effective_roots_from_plan_and_workspace;
    let manifest = runtime_fixture_project_root().join("runtime_semantics.bproj");
    let plan = build_compile_plan(&manifest, Some("NetworkNativeTests")).expect("real fixture plan");
    let declared = effective_roots_from_plan_and_workspace(&plan, None);
    let assembly = fixture("NetworkNativeTests");
    assert_eq!(assembly.roots.host, declared.host, "host root must be the checkout fixture source root");
    for root in &assembly.roots.dependencies {
        let checkout = root.source_root.canonicalize().expect("dependency root exists");
        assert!(
            declared.dependencies.iter().any(|declared| declared.source_root.canonicalize().ok().as_ref() == Some(&checkout)),
            "dependency root {root:?} is not a plan-declared checkout root"
        );
    }
    let local_state = plan.project_root.join("obj").canonicalize().ok();
    for unit in assembly.units.iter() {
        assert!(
            local_state.as_ref().is_none_or(|obj| !unit.path.starts_with(obj)),
            "fixture unit {} comes from a local materialized snapshot",
            unit.path.display()
        );
    }
    let results = assembly
        .units
        .iter()
        .filter(|unit| unit.path.ends_with("Core/Results/Results.bd"))
        .count();
    assert_eq!(results, 1, "exactly one Core.Results declaration enters the fixture assembly");
}

/// Regression (rc30 NetworkNativeTests): `beskid test` assembles the fixture over its prepared
/// workspace, whose Foundation root is the materialized copy under `obj/beskid/deps`. The canonical
/// Foundation support units were added at the checkout Foundation path beside that copy, so the
/// assembly declared `Core.Results` twice and Corelib Concurrency/Network units failed with
/// ResolveAmbiguousModulePath and unknown `Result`. The canonical units now replace the files of
/// the Foundation root the assembly actually reads. This prepares the real fixture's workspace
/// (gitignored `obj/` and `Project.lock`), exactly as the CLI does.
#[test]
fn prepared_workspace_fixture_assembles_one_foundation_copy() {
    use beskid_analysis::projects::{WorkspacePrepareOptions, prepare_project_workspace_with_options};
    let manifest = runtime_fixture_project_root().join("runtime_semantics.bproj");
    let plan = build_compile_plan(&manifest, Some("NetworkNativeTests")).expect("real fixture plan");
    let workspace = prepare_project_workspace_with_options(
        &plan,
        WorkspacePrepareOptions { offline: true, ..WorkspacePrepareOptions::default() },
        None,
    )
    .expect("prepared fixture workspace");
    let entry = plan_entry_path(&plan, &workspace.materialized_source_root);
    let source = std::fs::read_to_string(&entry).expect("materialized fixture source");
    let assembly = assemble_program_with_materializer(
        &plan,
        Some(&workspace),
        &entry,
        Some(&source),
        &assembly_options_for_plan(&plan),
        None,
        None,
    )
    .expect("prepared fixture assembly");
    assert!(assembly.runtime_fixture.is_some(), "the prepared fixture keeps its runtime proof");
    let foundation = assembly
        .roots
        .dependencies
        .iter()
        .find(|root| root.dependency_name.as_deref() == Some("corelib_foundation"))
        .expect("Foundation root")
        .source_root
        .clone();
    let results =
        assembly.units.iter().filter(|unit| unit.path.ends_with("Core/Results/Results.bd")).collect::<Vec<_>>();
    assert_eq!(results.len(), 1, "exactly one Core.Results declaration enters the prepared fixture assembly");
    assert!(results[0].path.starts_with(&foundation), "Core.Results comes from the assembled Foundation root");
    let mut db = BeskidDatabase::default();
    let findings = semantic_diagnostics_for_roots(&mut db, None, &assembly, &assembly.entry_unit().program, None)
        .expect("the query gate must judge the prepared fixture");
    let errors =
        findings.iter().filter(|finding| finding.kind.severity() == beskid_analysis::Severity::Error).collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn extra_runtime_source_cannot_extend_fixture_authority() {
    let mut assembly = fixture("LifecycleTests");
    let mut units = assembly.units.as_ref().clone();
    let mut extra = units.iter().find(|unit| unit.logical_name.starts_with("src/Runtime/")).unwrap().clone();
    extra.logical_name = "src/Runtime/Unauthorized.bd".into();
    extra.path = extra.path.with_file_name("Unauthorized.bd");
    units.push(extra);
    assembly.units = Arc::new(units);
    let mut db = BeskidDatabase::default();
    let project = project_session_for_syntax_assembly(&db, &assembly, "fixture", "fixture").unwrap();
    let generation = assembly.generation;
    assert!(
        build_runtime_fixture_typed_program(&mut db, project, generation, Arc::new(assembly), &manifest()).is_err()
    );
}

#[test]
fn absent_fixture_proof_cannot_authorize_an_ordinary_assembly() {
    let mut assembly = fixture("LifecycleTests");
    assembly.runtime_fixture = None;
    let mut db = BeskidDatabase::default();
    let project = project_session_for_syntax_assembly(&db, &assembly, "ordinary", "ordinary").unwrap();
    let generation = assembly.generation;
    assert!(
        build_runtime_fixture_typed_program(&mut db, project, generation, Arc::new(assembly), &manifest()).is_err()
    );
}

#[test]
fn additional_source_cannot_reuse_the_declared_fixture_identity() {
    let mut assembly = fixture("LifecycleTests");
    let mut units = assembly.units.as_ref().clone();
    let mut extra = assembly.entry_unit().clone();
    extra.path = extra.path.with_file_name("UndeclaredTests.bd");
    units.push(extra);
    assembly.units = Arc::new(units);
    let mut db = BeskidDatabase::default();
    let project = project_session_for_syntax_assembly(&db, &assembly, "fixture", "fixture").unwrap();
    let generation = assembly.generation;
    assert!(
        build_runtime_fixture_typed_program(&mut db, project, generation, Arc::new(assembly), &manifest()).is_err()
    );
}

#[test]
fn exact_fixture_gets_runtime_scope_without_granting_it_to_corelib() {
    let assembly = fixture("ExternalWorkTests");
    let fixture_path = assembly.entry_unit().logical_name.clone();
    let mut db = BeskidDatabase::default();
    let project = project_session_for_syntax_assembly(&db, &assembly, "fixture", "fixture").unwrap();
    let generation = assembly.generation;
    let typed = build_runtime_fixture_typed_program(&mut db, project, generation, Arc::new(assembly), &manifest())
        .expect("exact production corpus plus declared fixture");
    let capability = typed.runtime_intrinsic_capability.as_ref().unwrap();
    assert!(capability.authorizes_source(&fixture_path));
    assert!(capability.authorizes_source("src/Runtime/Network/Table.bd"));
    assert!(!capability.authorizes_source("src/Testing/Assert.bd"));
    assert!(!capability.authorizes_source("src/Runtime/Unauthorized.bd"));
}

#[test]
fn canonical_network_deadline_call_and_boolean_facts_preserve_signed_clock() {
    use beskid_analysis::syntax_query::NodeKind;
    use beskid_queries::{
        AstNodeKey, CallLowering, OperatorFact, SemanticTypeId, SourceUnitId, abi_type, call_abi_signature,
        call_lowering, operator_fact, value_abi_type,
    };
    let assembly = Arc::new(fixture("NetworkNativeTests"));
    let mut db = BeskidDatabase::default();
    let project = project_session_for_syntax_assembly(&db, &assembly, "deadline", "deadline").unwrap();
    build_runtime_fixture_typed_program(&mut db, project, assembly.generation, assembly.clone(), &manifest()).unwrap();
    let source = assembly
        .units
        .iter()
        .find(|unit| {
            unit.logical_name.ends_with("Runtime/Network/Operations.bd")
                || unit.path.ends_with("Runtime/Network/Operations.bd")
        })
        .expect("actual production Operations unit");
    let index = assembly.syntax_index_for_path(&source.path).unwrap();
    let unit = SourceUnitId::new(&db, source.path.clone());
    let mut clocks = 0;
    for node in index.ids_of_kind(NodeKind::CallExpression) {
        let key = AstNodeKey { unit, generation: assembly.generation, node };
        let call =
            index.node_at(&source.program, node).unwrap().of::<beskid_analysis::syntax::CallExpression>().unwrap();
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            continue;
        };
        if path.node.path.node.segments.len() == 1
            && path.node.path.node.segments[0].node.name.node.name == "clock_monotonic_nanos"
        {
            clocks += 1;
            assert!(
                matches!(call_lowering(&db, key).unwrap(), Some(CallLowering::Runtime(_))),
                "canonical clock intrinsic lowering"
            );
            assert_eq!(call_abi_signature(&db, key).unwrap().unwrap().result, SemanticTypeId::I64);
            assert_eq!(abi_type(&db, key).unwrap(), Some(SemanticTypeId::I64));
            assert_eq!(value_abi_type(&db, key).unwrap(), Some(SemanticTypeId::I64));
        }
    }
    assert!(clocks > 0, "actual runtime deadline clock call must be checked");
    let mut conditions = 0;
    for node in index.ids_of_kind(NodeKind::BinaryExpression) {
        let binary =
            index.node_at(&source.program, node).unwrap().of::<beskid_analysis::syntax::BinaryExpression>().unwrap();
        if binary.op.node != beskid_analysis::syntax::BinaryOp::And {
            continue;
        }
        // Concrete operand spans retain the full condition plus parser whitespace.
        let start = binary.left.span.start;
        let end = binary.right.span.end;
        if source.source[start..end].trim_end() == "effectiveDeadline >= 0 && effectiveDeadline <= clock_monotonic_nanos()" {
            conditions += 1;
            assert_eq!(binary.op.span.line_col_start.0, 83, "actual failing production deadline condition");
            let beskid_analysis::syntax::Expression::Binary(left) = &binary.left.node else {
                panic!("left comparison");
            };
            let beskid_analysis::syntax::Expression::Binary(right) = &binary.right.node else {
                panic!("right comparison");
            };
            assert_eq!(left.node.op.node, beskid_analysis::syntax::BinaryOp::Gte);
            assert_eq!(right.node.op.node, beskid_analysis::syntax::BinaryOp::Lte);
            let key = AstNodeKey { unit, generation: assembly.generation, node };
            assert_eq!(operator_fact(&db, key).unwrap(), Some(OperatorFact::And));
            assert_eq!(abi_type(&db, key).unwrap(), Some(SemanticTypeId::BOOL));
            assert_eq!(value_abi_type(&db, key).unwrap(), Some(SemanticTypeId::BOOL));
        }
    }
    assert_eq!(conditions, 1, "exact failed deadline condition must be tested once");
}
