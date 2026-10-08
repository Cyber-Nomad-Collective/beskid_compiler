//! Real runtime fixture assembly; no surrogate runtime implementation is used.
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::runtime_fixture_project_root;
use beskid_analysis::projects::{
    ProgramAssembly, assemble_program_with_materializer, assembly_options_for_plan, build_compile_plan, plan_entry_path,
};
use beskid_queries::{BeskidDatabase, build_runtime_fixture_typed_program, project_session_for_syntax_assembly};
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
        let result = beskid_analysis::services::type_entry_gate(assembly.entry_unit().program.clone(), &assembly);
        assert!(result.is_ok(), "{name}: {result:?}");
    }
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
