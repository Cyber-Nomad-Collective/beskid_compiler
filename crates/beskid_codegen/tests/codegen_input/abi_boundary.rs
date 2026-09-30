use super::support::{
    AbiManifestV5, Arc, AstNodeKey, CodegenInput, CodegenInputError, SyntaxGenerationId, TargetMetadata, input_fixture,
    input_fixture_with_source,
};

#[test]
fn sole_codegen_boundary_accepts_current_syntax_roots_and_exact_abi() {
    let (db, typed, root, target) = input_fixture();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let input = CodegenInput::new(&db, typed, Arc::from([root]), target, manifest).expect("valid codegen input");
    assert_eq!(input.roots(), &[root]);
}

#[test]
fn sole_codegen_boundary_rejects_stale_roots_and_manifest_drift() {
    let (db, typed, root, target) = input_fixture();
    let stale = AstNodeKey { generation: SyntaxGenerationId(0), ..root };
    assert!(matches!(
        CodegenInput::new(
            &db,
            typed.clone(),
            Arc::from([stale]),
            target.clone(),
            AbiManifestV5::canonical_runtime(target.clone()),
        ),
        Err(CodegenInputError::InvalidRoot(key)) if key == stale
    ));

    let other_target =
        TargetMetadata::supported().into_iter().find(|candidate| candidate != &target).expect("other target");
    assert!(matches!(
        CodegenInput::new(&db, typed, Arc::from([root]), target, AbiManifestV5::canonical_runtime(other_target),),
        Err(CodegenInputError::ManifestTargetMismatch)
    ));
}

#[test]
fn composition_plan_is_generation_bound_and_has_no_dynamic_fallback() {
    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Logger {}
type Worker { inject Logger[] loggers }
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (db, typed, root, target) = input_fixture_with_source(source);
    let generation = typed.generation;
    let resolved =
        resolve_composition(CompositionInput { program: &typed.assembly.entry_unit().program, is_mod_project: false });
    assert!(resolved.issues.is_empty(), "composition must validate: {:?}", resolved.issues);
    let mut snapshot = resolved.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(&db).to_path_buf());
    let plan = Arc::new(resolved.plan);
    let snapshot = Arc::new(snapshot);
    let owner = plan.plurals[0].owner_registration_id;
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("valid codegen input");
    assert!(input.composition_authority().is_none(), "ordinary codegen receives no lookup fallback");
    let input = input
        .with_composition_authority(generation, Arc::clone(&plan), Arc::clone(&snapshot))
        .expect("current-generation composition authority");
    assert_eq!(input.composition_authority(), Some((plan.as_ref(), snapshot.as_ref())));
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.composition_service_slot(owner), Some(1));
    assert_eq!(facts.composition_plural_slots(owner), Some(vec![0]));
    assert_eq!(facts.composition_service_slot(99), None, "unknown registrations fail closed");
}

#[test]
fn composition_plan_rejects_a_foreign_generation() {
    let (db, typed, root, target) = input_fixture();
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("valid codegen input");

    assert!(matches!(
        input.with_composition_authority(
            SyntaxGenerationId(999),
            Arc::new(beskid_analysis::composition::BindingPlan::default()),
            Arc::new(beskid_analysis::composition::CompositionSnapshot::default()),
        ),
        Err(CodegenInputError::StaleCompositionPlan)
    ));
}
