use super::support::{
    SyntaxModuleItem, find_function_definitions, find_node, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
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
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
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
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.activation.len(), 2);
    let generation = input.typed_program().generation;
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let snapshot = Arc::new(snapshot);

    let mut wrong_source = (*snapshot).clone();
    wrong_source.registrations[0].span = Default::default();
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-registration-source")).with_composition_authority(
            generation,
            Arc::new(result.plan.clone()),
            Arc::new(wrong_source)
        ),
        Err(CodegenInputError::InvalidCompositionPlan { .. })
    ));

    let mut foreign_registration = result.plan.clone();
    foreign_registration.activation[0].registration_id = 999;
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("foreign-registration")).with_composition_authority(
            generation,
            Arc::new(foreign_registration),
            Arc::clone(&snapshot)
        ),
        Err(CodegenInputError::InvalidCompositionPlan { .. })
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
        Err(CodegenInputError::InvalidCompositionPlan { .. })
    ));
    duplicate_slot.activation[1].slot = duplicate_slot.activation[0].slot;
    assert!(matches!(
        input.with_composition_authority(generation, Arc::new(duplicate_slot), snapshot),
        Err(CodegenInputError::InvalidCompositionPlan { .. })
    ));
}

#[test]
fn composition_authority_rejects_unplanned_injection_and_hook() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, ServiceSlot, resolve_composition};
    use beskid_codegen::CodegenInputError;

    let source = r#"
type Logger {}
type Worker { inject Logger logger; }
host AppHost() { registry { single Logger; single Worker; } startup() {} }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.singulars.len(), 1);
    assert_eq!(result.plan.startup_hooks.len(), 1);
    let generation = input.typed_program().generation;
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let snapshot = Arc::new(snapshot);

    let mut wrong_field = result.plan.clone();
    wrong_field.singulars[0].field_span = Default::default();
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-injection-field")).with_composition_authority(
            generation,
            Arc::new(wrong_field),
            Arc::clone(&snapshot)
        ),
        Err(CodegenInputError::InvalidCompositionPlan { .. })
    ));

    let mut wrong_target = result.plan.clone();
    wrong_target.singulars[0].target_slot = ServiceSlot(99);
    assert!(matches!(
        input.with_artifact_namespace(Arc::from("wrong-target")).with_composition_authority(
            generation,
            Arc::new(wrong_target),
            Arc::clone(&snapshot)
        ),
        Err(CodegenInputError::InvalidCompositionPlan { .. })
    ));

    let mut missing_hook = result.plan;
    let hook_span = missing_hook.startup_hooks[0].span;
    missing_hook.startup_hooks[0].span = beskid_analysis::syntax::SpanInfo { start: 0, end: 1, ..hook_span };
    let assembly = Arc::clone(&input.typed_program().assembly);
    let root_path = root.unit.path(input.database()).to_path_buf();
    let Err(error) = input.with_composition_authority(generation, Arc::new(missing_hook), snapshot) else {
        panic!("a hook that does not bind is rejected");
    };
    // The report names the declaration that did not bind, not only the failed check.
    let CodegenInputError::InvalidCompositionPlan { check, declaration: Some(declaration) } = &error else {
        panic!("an unbound hook names its declaration: {error}");
    };
    assert_eq!(*check, "hook nodes");
    assert_eq!(declaration.kind, beskid_queries::IndexedNodeKind::ScopeHook);
    assert_eq!(declaration.found, 0);
    assert_eq!(declaration.unit, root_path);
    let report = error.into_report(&assembly, "invalid composition codegen input");
    let bundle = report
        .downcast_ref::<beskid_analysis::services::SemanticDiagnosticsError>()
        .expect("the report carries a structured diagnostic");
    let [diagnostic] = bundle.diagnostics() else { panic!("one diagnostic: {:?}", bundle.diagnostics()) };
    assert_eq!(diagnostic.code.as_deref(), Some("E2105"), "{diagnostic:?}");
    assert_eq!(diagnostic.label, "this declaration does not bind");
    let help = diagnostic.help.as_deref().unwrap_or_default();
    assert!(help.contains("hook nodes") && help.contains("0 ScopeHook nodes"), "{help}");
}

#[test]
fn owner_only_plural_lookup_rejects_multiple_fields_until_source_keyed() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Logger {}
type Worker { inject Logger[] primary; inject Logger[] secondary; }
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
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
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
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
    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use beskid_isle::NodeFacts;
    use std::sync::Arc;

    let source = r#"
host AppHost() { scope Outer() { scope Inner() {} } }
i32 Main() { launch AppHost(); with Outer() { with Inner() {} } return 0; }
"#;
    let (input, _, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach exact authority");
    let scopes =
        super::support::find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::WithStatement);
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
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
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
fn failed_launch_branches_through_teardown_before_trapping() {
    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use std::sync::Arc;

    let source = "host AppHost() {} i32 Main() { launch AppHost(); return 0; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach exact authority");
    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("validated launch lowers");
    let clif = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("lowered Main")
        .function
        .display()
        .to_string();
    let (failed, active) = clif.split_once("trap user9").expect("failure path must trap after teardown");
    for symbol in ["composition_shutdown", "composition_container_drop"] {
        let callees = clif
            .lines()
            .filter(|line| line.contains(&format!("%{symbol}")))
            .filter_map(|line| line.split_whitespace().next())
            .map(|callee| format!("call {callee}("))
            .collect::<Vec<_>>();
        assert!(!callees.is_empty(), "missing canonical import {symbol}:\n{clif}");
        assert!(callees.iter().any(|call| failed.contains(call)), "failure must call {symbol} before trap:\n{clif}");
        assert!(callees.iter().any(|call| active.contains(call)), "normal return must call {symbol}:\n{clif}");
    }
}

#[test]
fn failed_slot_publication_and_object_allocation_branch_through_teardown() {
    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use std::sync::Arc;

    let source =
        "type Logger {} host AppHost() { registry { single Logger; } } i32 Main() { launch AppHost(); return 0; }";
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach exact authority");
    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("validated launch lowers");
    let clif = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("lowered Main")
        .function
        .display()
        .to_string();
    assert!(clif.contains("%composition_slot_store"), "registration must publish its slot:\n{clif}");
    for symbol in ["composition_shutdown", "composition_container_drop"] {
        let callees = clif
            .lines()
            .filter(|line| line.contains(&format!("%{symbol}")))
            .filter_map(|line| line.split_whitespace().next())
            .map(|callee| format!("call {callee}("))
            .collect::<Vec<_>>();
        let calls = callees.iter().map(|call| clif.matches(call).count()).sum::<usize>();
        assert!(
            calls >= 4,
            "allocation failure, slot-store failure, launch failure, and normal exit each need {symbol}:\n{clif}"
        );
    }
}

#[test]
fn structured_scope_exits_balance_return_break_and_continue() {
    use beskid_analysis::composition::{CompositionInput, resolve_composition};
    use std::sync::Arc;

    for (label, source, expected) in [
        (
            "nested return",
            "host AppHost() { scope Outer() { scope Inner() {} } } i32 Main() { launch AppHost(); with Outer() { with Inner() { return 0; } } }",
            2,
        ),
        (
            "loop break",
            "host AppHost() { scope Outer() {} } i32 Main() { launch AppHost(); while true { with Outer() { break; } } return 0; }",
            1,
        ),
        (
            "loop continue",
            "host AppHost() { scope Outer() {} } i32 Main() { launch AppHost(); while true { with Outer() { continue; } } return 0; }",
            1,
        ),
    ] {
        let (input, isa, root) = item_fixture_with_root(source);
        let result = resolve_composition(CompositionInput {
            program: &input.typed_program().assembly.entry_unit().program,
            is_mod_project: false,
            types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
        });
        assert!(result.issues.is_empty(), "{label}: composition must validate: {:?}", result.issues);
        let mut snapshot = result.snapshot;
        snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
        let generation = input.typed_program().generation;
        let input = input
            .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
            .expect("attach exact authority");
        let main = find_function_definitions(input.database(), root)
            .into_iter()
            .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
            .expect("Main function");
        let artifact =
            lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
                .expect("structured composition exit lowers to verified CLIF");
        let clif = artifact
            .functions
            .iter()
            .find(|function| function.name == "Main")
            .expect("lowered Main")
            .function
            .display()
            .to_string();
        for symbol in ["composition_scope_enter", "composition_scope_leave"] {
            let callees = clif
                .lines()
                .filter(|line| line.contains(&format!("%{symbol}")))
                .filter_map(|line| line.split_whitespace().next())
                .map(|callee| format!("call {callee}("))
                .collect::<Vec<_>>();
            let calls = callees.iter().map(|call| clif.matches(call).count()).sum::<usize>();
            assert_eq!(calls, expected, "{label}: each entered scope needs one {symbol}:\n{clif}");
        }
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
type Worker { inject Logger primary; inject Logger[] allLoggers; }
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.activation.len(), 2);
    assert_eq!(result.plan.singulars.len(), 1);
    assert_eq!(result.plan.plurals.len(), 1);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen authority");
    let (bound_plan, bound_snapshot) = input.composition_authority().expect("bound composition authority");
    for registration in &bound_snapshot.registrations {
        let site = beskid_queries::AstNodeKey { unit: root.unit, generation, node: registration.source_node_id };
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
    for field in bound_plan
        .singulars
        .iter()
        .map(|entry| entry.field_node_id)
        .chain(bound_plan.plurals.iter().map(|entry| entry.field_node_id))
    {
        let key = beskid_queries::AstNodeKey { unit: root.unit, generation, node: field };
        assert!(
            beskid_queries::composition_injection_field(input.database(), key).expect("injection query").is_some(),
            "validated injection field must have an exact source key"
        );
    }
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
    for symbol in ["composition_shutdown", "composition_container_drop"] {
        let callees = clif
            .lines()
            .filter(|line| line.contains(&format!("%{symbol}")))
            .filter_map(|line| line.split_whitespace().next())
            .map(|callee| format!("call {callee}("))
            .collect::<Vec<_>>();
        let calls = callees.iter().map(|call| clif.matches(call).count()).sum::<usize>();
        // Plural injection arrays are initialized without a barrier (0.6.2), so no barrier
        // failure path remains.
        assert!(
            calls >= 7,
            "both object and slot failures, plural allocation failure, launch failure, and normal exit need {symbol}:\n{clif}"
        );
        assert!(!clif.contains("beskid_rt_v5_array_write_barrier"), "no array barrier for injection arrays:\n{clif}");
    }
}

#[test]
fn parsed_injected_field_read_uses_frozen_physical_slot() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Logger {}
type Worker { inject Logger primary; }
host AppHost() { registry { single Logger; single Worker; } }
Logger Read(Worker worker) { return worker.primary; }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
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

/// Count the calls of the imported function whose declaration names `symbol`.
fn calls_of(clif: &str, symbol: &str) -> usize {
    clif.lines()
        .filter(|line| line.contains(&format!("%{symbol}")))
        .filter_map(|line| line.split_whitespace().next())
        .map(|callee| clif.matches(&format!("call {callee}(")).count())
        .sum()
}

#[test]
fn launch_runs_startup_and_with_runs_init_and_dispose_on_every_exit() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, is_hook_item_name, resolve_composition};

    let source = r#"
type Greeter {}
host AppHost()
{
    registry { single Greeter; }
    startup(Greeter greeter) { }
    dispose() { }
    scope Visit()
    {
        init(Greeter greeter) { }
        dispose() { }
    }
}
i32 Main(bool early)
{
    launch AppHost();
    with Visit()
    {
        if early { return 1; }
    }
    return 0;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert_eq!(result.plan.startup_hooks.len(), 1);
    assert_eq!(result.plan.init_hooks.len(), 1);
    assert_eq!(result.plan.disposal_hooks.len(), 2);
    assert!(result.plan.startup_hooks[0].arguments_registration_id.is_some(), "startup takes an injected argument");
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("hook functions and argument registrations are exact authority");
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();
    let hooks = items.iter().map(|item| item.symbol.clone()).filter(|name| is_hook_item_name(name)).collect::<Vec<_>>();
    assert_eq!(hooks.len(), 4, "one generated function per hook: {hooks:?}");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items).expect("hook calls lower to verified CLIF");
    let clif = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("lowered Main")
        .function
        .display()
        .to_string();
    let named = |kind: &str| {
        hooks
            .iter()
            .filter(|name| name.contains(&format!("_{kind}_")))
            .map(|name| (name.clone(), calls_of(&clif, name)))
            .collect::<Vec<_>>()
    };
    let startup = named("startup");
    assert_eq!(startup.len(), 1);
    assert_eq!(startup[0].1, 1, "startup runs once at launch:\n{clif}");
    let init = named("init");
    assert_eq!(init[0].1, 1, "init runs once per with entry:\n{clif}");
    let dispose = named("dispose");
    assert_eq!(dispose.len(), 2);
    // The scope dispose runs on the early return and on the normal end of the body; the global
    // dispose runs on both function exits before the container shuts down.
    for (name, calls) in &dispose {
        assert_eq!(*calls, 2, "{name} must run on both exits:\n{clif}");
    }
    assert_eq!(calls_of(&clif, "composition_launch"), 1);
}

/// Every hook kind takes a contract-typed and a concrete service from the registry, and a body
/// that omits an optional argument (typing fills it, which adds syntax nodes before the generated
/// argument types) still binds the frozen plan to the lowered syntax.
#[test]
fn every_hook_kind_takes_contract_and_concrete_services_from_the_registry() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, is_hook_item_name, resolve_composition};

    let source = r#"
unit Note(string text, [CallerLineNumber] i64 line = 0) { return; }
unit Early() { Note("early"); }
contract Greeter { string Greet(string name); }
type PoliteGreeter : Greeter { pub string Greet(string name) { return "Good morning"; } }
type Reception
{
    inject Greeter greeter;
    pub string Welcome(string name) { return this.greeter.Greet(name); }
}
host AppHost()
{
    registry { single PoliteGreeter for Greeter; single Reception; }
    startup(Greeter greeter, Reception reception) { Note(greeter.Greet("a")); Note(reception.Welcome("b")); }
    init(Greeter greeter, Reception reception) { Note(greeter.Greet("c")); Note(reception.Welcome("d")); }
    dispose(Greeter greeter, Reception reception) { Note(greeter.Greet("e")); Note(reception.Welcome("f")); }
    scope Visit()
    {
        init(Greeter greeter, Reception reception) { Note(greeter.Greet("g")); Note(reception.Welcome("h")); }
        dispose(Greeter greeter, Reception reception) { Note(greeter.Greet("i")); Note(reception.Welcome("j")); }
    }
}
i32 Main()
{
    Early();
    launch AppHost();
    with Visit() { }
    return 0;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let hooks = result.plan.startup_hooks.iter().chain(&result.plan.init_hooks).chain(&result.plan.disposal_hooks);
    assert_eq!(hooks.clone().count(), 5);
    assert!(hooks.clone().all(|hook| hook.arguments_registration_id.is_some()), "every hook takes injected services");
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("the frozen plan binds to the lowered syntax");
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition]
    {
        for key in find_nodes_of_kind(input.database(), root, kind) {
            let symbol = item_name(input.database(), key).expect("item query").expect("item name").to_string();
            items.push(SyntaxModuleItem { symbol, key });
        }
    }
    let hooks = items.iter().map(|item| item.symbol.clone()).filter(|name| is_hook_item_name(name)).collect::<Vec<_>>();
    assert_eq!(hooks.len(), 5, "one generated function per hook: {hooks:?}");
    let artifact =
        lower_syntax_program(&input, isa.as_ref(), &items).expect("hooks with services lower to verified CLIF");
    beskid_codegen::validate_artifact(&artifact).expect("hook artifact");
    let clif_of = |name: &str| {
        artifact
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("lowered function")
            .function
            .display()
            .to_string()
    };
    let main = clif_of("Main");
    for kind in ["_startup_", "_init_"] {
        for hook in hooks.iter().filter(|name| name.contains(kind)) {
            assert!(calls_of(&main, hook) >= 1, "{hook} runs from Main:\n{main}");
        }
    }
    for hook in &hooks {
        let body = clif_of(hook);
        assert!(body.matches("call").count() >= 4, "{hook} calls both services and Note twice:\n{body}");
    }
}

#[test]
fn hooks_receive_the_launch_and_with_arguments_as_records() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, is_hook_item_name, resolve_composition};

    let source = r#"
type Greeter {}
host AppHost(i64 count)
{
    registry { single Greeter; }
    startup(Greeter greeter) { Note(count); }
    scope Visit(i64 id)
    {
        init() { Note(id); }
        dispose() { Note(id); }
    }
}
unit Note(i64 value) { }
i32 Main(bool early)
{
    launch AppHost(3);
    with Visit(7)
    {
        if early { return 1; }
    }
    return 0;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    assert!(result.plan.startup_hooks.iter().chain(&result.plan.init_hooks).all(|hook| hook.takes_arguments_record));
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("hook functions and argument registrations are exact authority");
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();
    let artifact =
        lower_syntax_program(&input, isa.as_ref(), &items).expect("hook calls with records lower to verified CLIF");
    let clif = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("lowered Main")
        .function
        .display()
        .to_string();
    let hooks = items.iter().map(|item| item.symbol.clone()).filter(|name| is_hook_item_name(name)).collect::<Vec<_>>();
    for kind in ["startup", "init"] {
        let name = hooks.iter().find(|name| name.contains(&format!("_{kind}_"))).expect("hook function");
        assert_eq!(calls_of(&clif, name), 1, "{kind} runs once with its record:\n{clif}");
    }
    let dispose = hooks.iter().find(|name| name.contains("_dispose_")).expect("scope dispose");
    assert_eq!(calls_of(&clif, dispose), 2, "scope dispose runs on both exits:\n{clif}");
}

#[test]
fn dispose_hooks_are_registered_for_the_host_exit() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
type Greeter {}
host AppHost()
{
    registry { single Greeter; }
    dispose(Greeter greeter) { }
    scope Visit()
    {
        init() { }
        dispose() { }
    }
}
i32 Main()
{
    launch AppHost();
    with Visit() { }
    return 0;
}
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("composition authority");
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();
    let artifact =
        lower_syntax_program(&input, isa.as_ref(), &items).expect("exit registrations lower to verified CLIF");
    let clif = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("lowered Main")
        .function
        .display()
        .to_string();
    // One registration for the global dispose after launch, one for the scope dispose after init.
    assert_eq!(calls_of(&clif, "composition_exit_register"), 2, "{clif}");
    // Each direct dispose call withdraws its registration first, so `CommandLine.Exit` from inside
    // the hook does not run it a second time.
    assert!(calls_of(&clif, "composition_exit_pop") >= 2, "{clif}");
    // Every call imports its callee afresh (`fn23 = %composition_exit_pop`), so a symbol has
    // several references.
    let pops = callee_refs(&clif, "composition_exit_pop");
    assert!(!pops.is_empty(), "composition_exit_pop is imported:\n{clif}");
    let disposes = items
        .iter()
        .map(|item| item.symbol.as_str())
        .filter(|name| beskid_analysis::composition::is_hook_item_name(name) && name.contains("_dispose_"))
        .collect::<Vec<_>>();
    assert_eq!(disposes.len(), 2, "global and scope dispose functions: {disposes:?}");
    let calls = clif.lines().map(str::trim).filter(|line| line.contains("call ")).collect::<Vec<_>>();
    let calls_any = |line: &str, refs: &[String]| refs.iter().any(|callee| line.contains(&format!("call {callee}(")));
    let mut direct_calls = 0;
    for dispose in disposes {
        let refs = callee_refs(&clif, dispose);
        let mut seen = 0;
        for (index, line) in calls.iter().enumerate() {
            if calls_any(line, &refs) {
                seen += 1;
                assert!(
                    index > 0 && calls_any(calls[index - 1], &pops),
                    "{dispose} is called right after composition_exit_pop:\n{clif}"
                );
            }
        }
        assert!(seen >= 1, "{dispose} runs on the ordinary exit:\n{clif}");
        direct_calls += seen;
    }
    assert_eq!(direct_calls, calls_of(&clif, "composition_exit_pop"), "one pop per direct dispose call:\n{clif}");
}

/// Every CLIF function reference (`fn3`) that `symbol` is imported as.
fn callee_refs(clif: &str, symbol: &str) -> Vec<String> {
    clif.lines()
        .map(str::trim)
        .filter(|line| line.contains(&format!("= %{symbol} ")) || line.ends_with(&format!("= %{symbol}")))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_hook_with_ten_injected_services_takes_its_argument_object() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, is_hook_item_name, resolve_composition};

    let services = (0..10).map(|index| format!("S{index}")).collect::<Vec<_>>();
    let types = services.iter().map(|name| format!("type {name} {{}}\n")).collect::<String>();
    let registry = services.iter().map(|name| format!("single {name}; ")).collect::<String>();
    let parameters =
        services.iter().map(|name| format!("{name} {}", name.to_lowercase())).collect::<Vec<_>>().join(", ");
    let source = format!(
        r#"
{types}unit Use(S9 last) {{ }}
host AppHost(i64 count)
{{
    registry {{ {registry}}}
    startup({parameters}) {{ Use(s9); }}
    dispose({parameters}) {{ Use(s9); }}
    scope Visit()
    {{
        dispose({parameters}) {{ Use(s9); }}
    }}
}}
i32 Main()
{{
    launch AppHost(1);
    with Visit() {{ }}
    return 0;
}}
"#
    );
    let (input, isa, root) = item_fixture_with_root(&source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("composition authority");
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem {
            symbol: item_name(input.database(), key).expect("item query").expect("item name").to_string(),
            key,
        })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("hooks with ten services lower: each takes its record and argument object");
    let clif = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("lowered Main")
        .function
        .display()
        .to_string();
    let hooks = items.iter().map(|item| item.symbol.clone()).filter(|name| is_hook_item_name(name)).collect::<Vec<_>>();
    let startup = hooks.iter().find(|name| name.contains("_startup_")).expect("startup function");
    assert_eq!(calls_of(&clif, startup), 1, "{clif}");
    // Both dispose hooks are registered for the host exit, each with at most two arguments.
    assert_eq!(calls_of(&clif, "composition_exit_register"), 2, "{clif}");
}

/// Contract method calls on contract-typed receivers that are not parameters: an injected field
/// read as `this.greeter`, as a bare `greeter`, as `(this).greeter` and through a host value, and
/// an immutable local initialized with a concrete type. Each calls the one implementation
/// statically (the composition plan binds the injected field to `Hello`).
#[test]
fn contract_calls_on_injected_fields_and_locals_call_the_bound_implementation() {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let source = r#"
contract Greeter { string Greet(string name); }
type Hello : Greeter { pub string Greet(string name) { return "hi " + name; } }
type Worker {
    inject Greeter greeter;
    pub string ViaThis(string name) { return this.greeter.Greet(name); }
    pub string ViaField(string name) { return greeter.Greet(name); }
    pub string ViaGrouped(string name) { return (this).greeter.Greet(name); }
}
string ViaHost(Worker worker) { return worker.greeter.Greet("w"); }
string ViaLocal() { Greeter g = Hello {}; return g.Greet("l"); }
host AppHost() { registry { single Hello for Greeter; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#;
    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen composition authority");
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition]
    {
        for key in find_nodes_of_kind(input.database(), root, kind) {
            let symbol = item_name(input.database(), key).expect("item query").expect("item name").to_string();
            items.push(SyntaxModuleItem { symbol, key });
        }
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("contract calls on injected fields and contract-typed locals lower");
    beskid_codegen::validate_artifact(&artifact).expect("contract receiver artifact");
    for caller in ["ViaThis", "ViaField", "ViaGrouped", "ViaHost", "ViaLocal"] {
        let function = artifact.functions.iter().find(|function| function.name == caller).expect("caller lowered");
        let clif = function.function.display().to_string();
        assert!(clif.contains("call "), "`{caller}` calls the implementation directly:\n{clif}");
    }
}

/// Lower every function and method of `source` with its frozen composition plan attached.
fn lower_composed(source: &str) -> beskid_codegen::CodegenArtifact {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let (input, isa, root) = item_fixture_with_root(source);
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen composition authority");
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition]
    {
        for key in find_nodes_of_kind(input.database(), root, kind) {
            let symbol = item_name(input.database(), key).expect("item query").expect("item name").to_string();
            items.push(SyntaxModuleItem { symbol, key });
        }
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items).expect("composed contract values lower");
    beskid_codegen::validate_artifact(&artifact).expect("composed contract value artifact");
    artifact
}

/// `inject Greeter[] all` with two registered implementations: activation boxes each
/// implementation into the array with its own witness table, and a loop over the elements
/// dispatches every call through the table.
#[test]
fn plural_contract_injection_boxes_every_implementation_and_dispatches_in_a_loop() {
    let artifact = lower_composed(
        r#"
contract Greeter { i64 Rank(); }
type Hello : Greeter { pub i64 Rank() { return 1_i64; } }
type Bye : Greeter { pub i64 Rank() { return 2_i64; } }
type Worker {
    inject Greeter[] all;
    pub i64 Total() {
        mut i64 total = 0_i64;
        mut i64 i = 0_i64;
        while i < 2_i64 { total = total + all[i].Rank(); i = i + 1_i64; }
        return total;
    }
}
host AppHost() { registry { single Hello for Greeter; single Bye for Greeter; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#,
    );
    let total = artifact.functions.iter().find(|function| function.name == "Total").expect("Total lowered");
    let total = total.function.display().to_string();
    assert!(total.contains("call_indirect"), "each element dispatches through its table:\n{total}");
    assert_eq!(artifact.witness_tables.len(), 2, "Hello and Bye: {:?}", artifact.witness_tables);
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main lowered");
    let main = main.function.display().to_string();
    assert!(
        main.contains("__beskid_contract_box_reference_request_v5"),
        "activation boxes the implementations:\n{main}"
    );
}

/// A singular `inject Greeter greeter` whose binding differs between the host registry and a
/// scope: the field holds a box whose table is the binding of the activating scope, so the call
/// dispatches at run time instead of naming one implementation.
#[test]
fn a_singular_contract_injection_bound_differently_per_scope_dispatches_at_run_time() {
    let artifact = lower_composed(
        r#"
contract Greeter { i64 Rank(); }
type Hello : Greeter { pub i64 Rank() { return 1_i64; } }
type Bye : Greeter { pub i64 Rank() { return 2_i64; } }
type Worker {
    inject Greeter greeter;
    pub i64 Rank() { return greeter.Rank(); }
}
host AppHost() {
    registry { single Hello for Greeter; transient Worker; }
    scope Request() { single Bye for Greeter; transient Worker; }
}
i32 Main() { launch AppHost(); with Request() { return 0; } }
"#,
    );
    let rank = artifact
        .functions
        .iter()
        .find(|function| function.name.ends_with("Rank") && function.function.display().to_string().contains("+16"))
        .map(|function| function.function.display().to_string())
        .unwrap_or_else(|| {
            panic!("Worker.Rank lowered: {:?}", artifact.functions.iter().map(|f| f.name.as_str()).collect::<Vec<_>>())
        });
    assert!(rank.contains("call_indirect"), "two bindings: the call dispatches through the box:\n{rank}");
    assert_eq!(artifact.witness_tables.len(), 2, "Hello and Bye: {:?}", artifact.witness_tables);
}

/// A `mut` contract-typed local may hold any implementation, so a contract call on it resolves
/// to the contract method and dispatches through the box's witness table (runtime contract
/// dispatch, v0.6.7).
#[test]
fn contract_call_on_a_mutable_contract_local_dispatches_through_the_witness_table() {
    let (input, _, root) = item_fixture_with_root(
        "contract Greeter { string Greet(); } \
         type Hello : Greeter { pub string Greet() { return \"hi\"; } } \
         string Main() { mut Greeter g = Hello {}; return g.Greet(); }",
    );
    let call = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .next()
        .expect("the call");
    let lowering = beskid_queries::call_lowering(input.database(), call).expect("a mut contract local resolves");
    let Some(beskid_queries::CallLowering::Direct(declaration)) = lowering else {
        panic!("the call names the contract method: {lowering:?}");
    };
    assert_eq!(
        beskid_queries::node_kind(input.database(), declaration).expect("kind"),
        Some(beskid_queries::IndexedNodeKind::ContractMethodSignature)
    );
    let dispatch = beskid_queries::contract_dispatch(input.database(), call, None)
        .expect("dispatch query")
        .expect("a mut contract local dispatches at run time");
    assert_eq!(dispatch.slot, 0);
}
