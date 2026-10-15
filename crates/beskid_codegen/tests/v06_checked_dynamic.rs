//! Actual canonical source factories, not name-only no-yield admissions.
#[path = "isle_adapter/support.rs"]
mod support;
use support::*;
fn canonical() -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().unwrap().keep();
    let generation = SyntaxGenerationId(91);
    let (assembly, path) = canonical_runtime_test_assembly(&mut db, &directory, generation);
    let project = ProjectSession::new(&*db, directory, path, "checked-dynamic".into(), "canonical".into());
    let target =
        TargetMetadata::supported().into_iter().find(|t| t.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let capability = canonical_runtime_intrinsic_capability(&manifest).unwrap();
    let typed =
        build_canonical_runtime_typed_program(&mut db, project, generation, assembly.clone(), capability).unwrap();
    let roots = assembly
        .units
        .iter()
        .map(|unit| AstNodeKey { unit: SourceUnitId::new(&*db, unit.path.clone()), generation, node: AstNodeId(0) })
        .collect::<Vec<_>>();
    let db = Box::leak(db);
    let input = CodegenInput::new(db, typed, roots.into(), target, manifest).unwrap();
    let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
    (input, isa)
}
fn function(input: &CodegenInput<'_>, path: &str, name: &str) -> AstNodeKey {
    let unit = input.typed_program().assembly.units.iter().find(|unit| unit.logical_name == path).unwrap();
    let root = AstNodeKey {
        unit: SourceUnitId::new(input.database(), unit.path.clone()),
        generation: input.typed_program().generation,
        node: AstNodeId(0),
    };
    named_function(input, root, name)
}
#[test]
fn canonical_create_cast_map_result_factories_have_closed_checked_source_bodies() {
    let (input, isa) = canonical();
    for name in ["DynamicCreateResultFactoryV1", "DynamicCastResultFactoryV1", "DynamicMapResultFactoryV1"] {
        let key = function(&input, CANONICAL_DYNAMIC_SOURCE_PATH, name);
        let proof = input.checked_effect_closure(isa.as_ref(), key, None).unwrap();
        assert!(proof.members().count() >= 2, "owned bridge and source Result constructors are retained");
        assert!(proof.required_providers().any(|symbol| symbol.ends_with("_owned")));
        assert!(
            input.nonallocating_publication(isa.as_ref(), key, None).is_err(),
            "a checked Result factory is not allocation-free Commit/Abort"
        );
    }
}
#[test]
fn canonical_factory_stale_generation_is_rejected() {
    let (input, isa) = canonical();
    let key = function(&input, CANONICAL_DYNAMIC_SOURCE_PATH, "DynamicCreateResultFactoryV1");
    let stale = AstNodeKey { generation: SyntaxGenerationId(key.generation.0 + 1), ..key };
    assert!(input.checked_effect_closure(isa.as_ref(), stale, None).is_err());
}
#[test]
fn matching_owned_c_names_without_canonical_source_do_not_grant_effect() {
    let (input, isa, root) = item_fixture_with_root(
        "[Extern(Abi:\"C\", Library:\"beskid_runtime\")] contract DynamicBridgeV1 { pointer beskid_dynamic_v1_map_owned(pointer value, u64 mapping, pointer status); } pointer Main(pointer value, u64 mapping, pointer status) { return DynamicBridgeV1.beskid_dynamic_v1_map_owned(value,mapping,status); }",
    );
    let key = find_function_definition(input.database(), root).unwrap();
    assert!(input.checked_effect_closure(isa.as_ref(), key, None).is_err());
}
#[test]
fn missing_callback_body_cannot_issue_guarded_counterpart() {
    let (input, isa, root) = item_fixture_with_root(
        "[Extern(Abi:\"C\", Library:\"beskid_runtime\")] contract Transform { pointer Map(pointer value); } pointer Main(pointer value) { return Transform.Map(value); }",
    );
    let key = find_function_definition(input.database(), root).unwrap();
    assert!(input.checked_effect_closure(isa.as_ref(), key, None).is_err());
}
