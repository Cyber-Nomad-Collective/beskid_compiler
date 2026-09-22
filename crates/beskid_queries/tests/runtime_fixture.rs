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
    assert!(!plan.has_std_dependency, "installed Std must not contaminate runtime module identities");
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
