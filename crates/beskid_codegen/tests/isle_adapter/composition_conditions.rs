use super::support::{
    SyntaxModuleItem, find_function_definitions, find_node, item_fixture_with_root, item_name, lower_syntax_program,
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
        stale_input.with_composition_authority(
            SyntaxGenerationId(generation.0 + 1),
            Arc::new(Default::default()),
            snapshot
        ),
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

    let mut wrong_source = (*snapshot).clone();
    wrong_source.registrations[0].source_node_id = beskid_queries::AstNodeId::INVALID;
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-registration-source")).with_composition_authority(
            generation,
            Arc::new(result.plan.clone()),
            Arc::new(wrong_source)
        ),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));

    let mut foreign_registration = result.plan.clone();
    foreign_registration.activation[0].registration_id = 999;
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("foreign-registration")).with_composition_authority(
            generation,
            Arc::new(foreign_registration),
            Arc::clone(&snapshot)
        ),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));

    let mut duplicate_slot = result.plan;
    let mut wrong_host = duplicate_slot.clone();
    wrong_host.launched_host = "OtherHost".into();
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-host")).with_composition_authority(
            generation,
            Arc::new(wrong_host),
            Arc::clone(&snapshot)
        ),
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

    let mut wrong_field = result.plan.clone();
    wrong_field.singulars[0].field_node_id = beskid_queries::AstNodeId::INVALID;
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-injection-field")).with_composition_authority(
            generation,
            Arc::new(wrong_field),
            Arc::clone(&snapshot)
        ),
        Err(CodegenInputError::InvalidCompositionPlan)
    ));

    let mut wrong_target = result.plan.clone();
    wrong_target.singulars[0].target_slot = ServiceSlot(99);
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-target")).with_composition_authority(
            generation,
            Arc::new(wrong_target),
            Arc::clone(&snapshot)
        ),
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
fn composition_queries_bind_launch_and_nested_scope_to_exact_source_keys() {
    let source = r#"
host AppHost() {
    scope Request() {}
}
i32 Main() {
    launch AppHost();
    with Request() { return 0; }
}
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let launch =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::LaunchStatement).expect("launch statement");
    let with_statement =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::WithStatement).expect("with statement");
    let launch_fact = beskid_queries::composition_launch(input.database(), launch)
        .expect("generation-bound launch query")
        .expect("validated launch fact");
    assert_eq!(launch_fact.host.as_ref(), "AppHost");
    assert_eq!(launch_fact.site, launch);
    let scope_fact = beskid_queries::composition_scope(input.database(), with_statement)
        .expect("generation-bound scope query")
        .expect("validated scope fact");
    assert_eq!(scope_fact.scope_name.as_ref(), "Request");
    assert_eq!(scope_fact.site, with_statement);
    assert_eq!(
        beskid_queries::node_kind(input.database(), scope_fact.body).expect("body kind"),
        Some(beskid_queries::IndexedNodeKind::Block)
    );

    let stale = beskid_queries::AstNodeKey {
        generation: beskid_queries::SyntaxGenerationId(launch.generation.0 + 1),
        ..launch
    };
    assert!(beskid_queries::composition_launch(input.database(), stale).expect("stale query").is_none());
    assert!(beskid_queries::composition_scope(input.database(), launch).expect("wrong kind").is_none());

    let (foreign_input, _, foreign_root) = item_fixture_with_root("i32 Main() { return 0; }");
    let foreign = beskid_queries::AstNodeKey { unit: foreign_root.unit, ..launch };
    assert!(beskid_queries::composition_launch(foreign_input.database(), foreign).expect("foreign query").is_none());

    let (unresolved_input, _, unresolved_root) =
        item_fixture_with_root("i32 Main() { launch MissingHost(); return 0; }");
    let unresolved =
        find_node(unresolved_input.database(), unresolved_root, beskid_queries::IndexedNodeKind::LaunchStatement)
            .expect("unresolved launch statement");
    assert_eq!(
        beskid_queries::composition_launch(unresolved_input.database(), unresolved)
            .expect("source-shape query")
            .expect("syntax fact; validation belongs to frozen authority")
            .host
            .as_ref(),
        "MissingHost"
    );
}

#[test]
fn composition_node_facts_require_exact_attached_authority() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use beskid_isle::NodeFacts;

    let source = r#"
host AppHost() { scope Request() {} }
i32 Main() { launch AppHost(); with Request() { return 0; } }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let launch =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::LaunchStatement).expect("launch statement");
    let scope =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::WithStatement).expect("scope statement");
    let unbound = beskid_codegen::SyntaxNodeFacts::new(&input);
    assert!(unbound.composition_launch(launch).is_none());
    assert!(unbound.composition_scope(scope).is_none());

    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let attached = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach exact authority");
    let facts = beskid_codegen::SyntaxNodeFacts::new(&attached);
    let launch_plan = facts.composition_launch(launch).expect("source-keyed launch plan");
    assert_eq!(launch_plan.site, launch);
    assert_eq!(launch_plan.slot_count, 0);
    assert!(launch_plan.registrations.is_empty());
    let scope_plan = facts.composition_scope(scope).expect("source-keyed scope plan");
    assert_eq!(scope_plan.site, scope);
    assert_ne!(scope_plan.scope_id, 0);
    assert_eq!(scope_plan.parent_scope_id, 0, "root scope must retain the frozen global parent");
    assert!(facts.composition_launch(scope).is_none());
    assert!(facts.composition_scope(launch).is_none());
}

#[test]
fn nested_composition_scope_facts_keep_the_frozen_parent_identity() {
    use std::sync::Arc;
    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use beskid_isle::NodeFacts;

    let source = r#"
host AppHost() { scope Outer() { scope Inner() {} } }
i32 Main() { launch AppHost(); with Outer() { with Inner() {} } return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input.with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach exact authority");
    let scopes = super::support::find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::WithStatement);
    assert_eq!(scopes.len(), 2);
    let facts = beskid_codegen::SyntaxNodeFacts::new(&input);
    let outer = facts.composition_scope(scopes[0]).expect("outer scope");
    let inner = facts.composition_scope(scopes[1]).expect("inner scope");
    assert_eq!(outer.parent_scope_id, 0);
    assert_eq!(inner.parent_scope_id, outer.scope_id);
    assert_ne!(inner.scope_id, outer.scope_id);
}

#[test]
fn composition_launch_query_keeps_source_shape_when_base_host_lives_in_another_unit() {
    use beskid_isle::NodeFacts;

    let source = "host AppHost() : ImportedBase {} i32 Main() { launch AppHost(); return 0; }";
    let (input, _, root) = item_fixture_with_root(source);
    let launch =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::LaunchStatement).expect("launch statement");
    let fact = beskid_queries::composition_launch(input.database(), launch)
        .expect("generation-bound source fact")
        .expect("source shape must not independently re-resolve the imported graph");
    assert_eq!(fact.host.as_ref(), "AppHost");
    assert!(beskid_codegen::SyntaxNodeFacts::new(&input).composition_launch(launch).is_none());
}

#[test]
fn parsed_launch_and_scope_lower_through_generated_isle_with_canonical_calls() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
host AppHost() { scope Request() {} }
i32 Main() {
    launch AppHost();
    with Request() { }
    return 0;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen authority");
    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("validated launch and with must lower through generated ISLE");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("lowered Main");
    let clif = main.function.display().to_string();
    for symbol in [
        "composition_container_create",
        "composition_launch",
        "composition_scope_enter",
        "composition_scope_leave",
        "composition_shutdown",
        "composition_container_drop",
    ] {
        assert!(clif.contains(symbol), "missing canonical composition call `{symbol}`:\n{clif}");
    }
}

#[test]
fn executable_composition_without_frozen_authority_fails_at_launch_site() {
    let source = "host AppHost() {} i32 Main() { launch AppHost(); return 0; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect_err("a source-shaped launch is not composition authority");
    let displayed = error.to_string();
    assert!(displayed.contains("LaunchStatement") || displayed.contains("MissingRuleOrFact"), "{displayed}");
}

#[test]
fn launch_materializes_registration_slots_and_static_injections_before_activation() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Logger {}
type Worker { inject Logger primary, inject Logger[] allLoggers }
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.activation.len(), 2);
    assert_eq!(result.plan.singulars.len(), 1);
    assert_eq!(result.plan.plurals.len(), 1);
    for registration in &result.snapshot.registrations {
        let site = beskid_queries::AstNodeKey {
            unit: root.unit,
            generation: input.typed_program().generation,
            node: registration.source_node_id,
        };
        assert_eq!(
            beskid_queries::node_kind(input.database(), site).expect("registration node kind"),
            Some(beskid_queries::IndexedNodeKind::RegistryEntry),
            "snapshot registration must name its syntax entry"
        );
        let fact = beskid_queries::composition_registration(input.database(), site)
            .expect("registration query")
            .expect("exact implementation declaration");
        assert_eq!(fact.implementation.as_ref(), registration.implementation);
        assert_eq!(fact.site, site);
    }
    for field in result
        .plan
        .singulars
        .iter()
        .map(|entry| entry.field_node_id)
        .chain(result.plan.plurals.iter().map(|entry| entry.field_node_id))
    {
        let key =
            beskid_queries::AstNodeKey { unit: root.unit, generation: input.typed_program().generation, node: field };
        assert!(
            beskid_queries::composition_injection_field(input.database(), key).expect("injection query").is_some(),
            "validated injection field must have an exact source key"
        );
    }
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen authority");
    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("registered host lowers through generated ISLE");
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("lowered Main");
    let clif = main.function.display().to_string();
    assert_eq!(clif.matches("composition_slot_store").count(), 2, "each registration must install one slot:\n{clif}");
    assert!(
        clif.contains("beskid_rt_v5_managed_object_allocate"),
        "registered services need exact managed allocation:\n{clif}"
    );
}

#[test]
fn parsed_injected_field_read_uses_frozen_physical_slot() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Logger {}
type Worker { inject Logger primary }
host AppHost() { registry { single Logger; single Worker; } }
Logger Read(Worker worker) { return worker.primary; }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen composition authority");
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("an injected field read must use the frozen source field identity");
    let read = artifact.functions.iter().find(|function| function.name == "Read").expect("Read lowered");
    assert!(read.function.display().to_string().contains("load.i64"), "injected pointer slot must be read");
}

#[test]
fn compound_integer_argument_retains_its_resolved_word_type_at_a_call_boundary() {
    let (input, isa, root) = item_fixture_with_root(
        "const ENTRY_MAX = 256; word Allocate(word size, word alignment) { return size; } word Main() { return \
         Allocate(ENTRY_MAX * 8, 8); }",
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
