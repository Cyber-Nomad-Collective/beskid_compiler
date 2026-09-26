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
    assert!(input.composition_authority().is_none(), "ordinary codegen receives no lookup fallback");

    let plan = Arc::new(beskid_analysis::composition::BindingPlan {
        launched_host: "AppHost".into(),
        activation: vec![beskid_analysis::composition::ActivationPlanEntry {
            registration_id: 41,
            slot: beskid_analysis::composition::ServiceSlot(0),
        }],
        singulars: Vec::new(),
        plurals: vec![beskid_analysis::composition::PluralPlan {
            owner_registration_id: 41,
            field_span: Default::default(),
            target_slots: vec![beskid_analysis::composition::ServiceSlot(0)],
        }],
        scope_parents: Default::default(),
        init_hooks: Vec::new(),
        startup_hooks: Vec::new(),
        disposal_hooks: Vec::new(),
    });
    let snapshot = Arc::new(beskid_analysis::composition::CompositionSnapshot {
        version: 1,
        launched_host: "AppHost".into(),
        source_unit_path: Some(root.unit.path(&db).to_path_buf()),
        launch_span: None,
        registrations: vec![beskid_analysis::composition::Registration {
            id: 41,
            scope_id: beskid_analysis::composition::ScopeId::GLOBAL,
            key: beskid_analysis::composition::RegistrationKey::SelfType("Logger".into()),
            implementation: "Logger".into(),
            lifetime: beskid_analysis::composition::RegistrationLifetime::Single,
            span: Default::default(),
        }],
        scope_names: Default::default(),
    });
    let input = input
        .with_composition_authority(generation, Arc::clone(&plan), Arc::clone(&snapshot))
        .expect("current-generation composition authority");
    assert_eq!(input.composition_authority(), Some((plan.as_ref(), snapshot.as_ref())));
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert_eq!(facts.composition_service_slot(41), Some(0));
    assert_eq!(facts.composition_plural_slots(41), Some(vec![0]));
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
