use super::support::{
    SyntaxModuleItem, find_function_definitions, item_fixture_with_root, item_name, lower_syntax_program,
};

#[test]
fn parsed_composition_authority_binds_exact_host_registration_and_generation() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use beskid_codegen::CodegenInputError;
    use beskid_queries::SyntaxGenerationId;

    let source = r#"
host AppHost() { registry { single Logger; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, _isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let plan = Arc::new(result.plan);
    let snapshot = Arc::new(snapshot);
    let generation = input.typed_program().generation;

    let current = input
        .with_composition_authority(generation, Arc::clone(&plan), Arc::clone(&snapshot))
        .expect("exact source generation and plan attach");
    let (attached_plan, attached_snapshot) = current.composition_authority().expect("composition authority");
    assert_eq!(attached_snapshot.launched_host, "AppHost");
    assert_eq!(attached_snapshot.registrations[0].implementation, "Logger");
    assert_eq!(attached_plan.activation[0].registration_id, attached_snapshot.registrations[0].id);
    assert_eq!(attached_plan.activation[0].slot.0, 0);

    let (foreign_input, _, foreign_root) = item_fixture_with_root(source);
    let mut foreign = (*snapshot).clone();
    foreign.source_unit_path = Some(foreign_root.unit.path(foreign_input.database()).to_path_buf());
    assert!(matches!(
        current.with_composition_authority(generation, plan, Arc::new(foreign)),
        Err(CodegenInputError::ForeignCompositionUnit)
    ));

    let (stale_input, _, _) = item_fixture_with_root(source);
    assert!(matches!(
        stale_input.with_composition_authority(SyntaxGenerationId(generation.0 + 1), Arc::new(Default::default()), snapshot),
        Err(CodegenInputError::StaleCompositionPlan)
    ));
}

#[test]
fn composition_authority_rejects_forged_registration_and_duplicate_slot() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use beskid_codegen::CodegenInputError;

    let source = r#"
host AppHost() { registry { single Logger; single Metrics; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.activation.len(), 2);
    let generation = input.typed_program().generation;
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let snapshot = Arc::new(snapshot);

    let mut foreign_registration = result.plan.clone();
    foreign_registration.activation[0].registration_id = 999;
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("foreign-registration"))
            .with_composition_authority(generation, Arc::new(foreign_registration), Arc::clone(&snapshot)),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));

    let mut duplicate_slot = result.plan;
    let mut wrong_host = duplicate_slot.clone();
    wrong_host.launched_host = "OtherHost".into();
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-host"))
            .with_composition_authority(generation, Arc::new(wrong_host), Arc::clone(&snapshot)),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));
    duplicate_slot.activation[1].slot = duplicate_slot.activation[0].slot;
    assert!(matches!(
        input.with_composition_authority(generation, Arc::new(duplicate_slot), snapshot),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));
}

#[test]
fn composition_authority_rejects_unplanned_injection_and_hook() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, ServiceSlot, resolve_composition};
    use beskid_codegen::CodegenInputError;

    let source = r#"
type Logger {}
type Worker { inject Logger logger }
host AppHost() { registry { single Logger; single Worker; } startup() {} }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.singulars.len(), 1);
    assert_eq!(result.plan.startup_hooks.len(), 1);
    let generation = input.typed_program().generation;
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let snapshot = Arc::new(snapshot);

    let mut wrong_target = result.plan.clone();
    wrong_target.singulars[0].target_slot = ServiceSlot(99);
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-target"))
            .with_composition_authority(generation, Arc::new(wrong_target), Arc::clone(&snapshot)),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));

    let mut missing_hook = result.plan;
    missing_hook.startup_hooks[0].source_node_id = beskid_queries::AstNodeId::INVALID;
    assert!(matches!(
        input.with_composition_authority(generation, Arc::new(missing_hook), snapshot),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));
}

#[test]
fn owner_only_plural_lookup_rejects_multiple_fields_until_source_keyed() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Logger {}
type Worker { inject Logger[] primary, inject Logger[] secondary }
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.plurals.len(), 2);
    let owner = result.plan.plurals[0].owner_registration_id;
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let attached = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("validated composition authority");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&attached);
    assert_eq!(facts.composition_plural_slots(owner), None, "owner-only lookup is ambiguous across fields");
}

#[test]
fn compound_integer_argument_retains_its_resolved_word_type_at_a_call_boundary() {
    let (input, isa, root) = item_fixture_with_root(
        "const ENTRY_MAX = 256; word Allocate(word size, word alignment) { return size; } word Main() { return Allocate(ENTRY_MAX * 8, 8); }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item name query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();

    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a compound word argument retains its resolved type at an exact call boundary");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("lowered Main function");
    let clif = main.function.display().to_string();

    assert!(clif.contains("imul"), "the compound size expression must lower:\n{clif}");
    assert!(clif.contains("call"), "the direct call must lower:\n{clif}");
}
