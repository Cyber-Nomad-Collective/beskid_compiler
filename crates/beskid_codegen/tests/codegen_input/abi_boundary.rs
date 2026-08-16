use super::support::{
    AbiManifestV5, Arc, AstNodeKey, CodegenInput, CodegenInputError, SyntaxGenerationId, TargetMetadata, input_fixture,
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
    let (db, typed, root, target) = input_fixture();
    let generation = typed.generation;
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("valid codegen input");
    assert!(input.composition_plan().is_none(), "ordinary codegen receives no lookup fallback");

    let plan = Arc::new(beskid_analysis::composition::BindingPlan {
        activation: vec![beskid_analysis::composition::ActivationPlanEntry {
            registration_id: 41,
            slot: beskid_analysis::composition::ServiceSlot(0),
        }],
        plurals: vec![beskid_analysis::composition::PluralPlan {
            owner_registration_id: 42,
            target_slots: vec![beskid_analysis::composition::ServiceSlot(0)],
        }],
        scope_parents: Default::default(),
    });
    let input =
        input.with_composition_plan(generation, Arc::clone(&plan)).expect("current-generation composition plan");
    assert_eq!(input.composition_plan(), Some(plan.as_ref()));
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.composition_service_slot(41), Some(0));
    assert_eq!(facts.composition_plural_slots(42), Some(vec![0]));
    assert_eq!(facts.composition_service_slot(99), None, "unknown registrations fail closed");
}

#[test]
fn composition_plan_rejects_a_foreign_generation() {
    let (db, typed, root, target) = input_fixture();
    let input =
        CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("valid codegen input");

    assert!(matches!(
        input.with_composition_plan(
            SyntaxGenerationId(999),
            Arc::new(beskid_analysis::composition::BindingPlan::default()),
        ),
        Err(CodegenInputError::StaleCompositionPlan)
    ));
}
