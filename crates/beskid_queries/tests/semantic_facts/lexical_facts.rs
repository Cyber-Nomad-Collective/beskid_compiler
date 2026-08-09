use super::support::{key, key_at_start, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{LocalSlot, SyntaxGenerationId, control_flow, local_slot, node_type, resolved_local};

#[test]
fn control_flow_facts_follow_ast_branch_termination() {
    let source = r#"
i32 AlwaysReturns(bool condition) {
    if condition { return 1; } else { return 2; }
}
i32 MayFallThrough(bool condition) {
    if condition { return 1; }
}
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let always_returns = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let may_fall_through = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);

    assert_eq!(
        control_flow(&db, always_returns).expect("always-returning flow"),
        Some(beskid_queries::ControlFlow { may_fall_through: false })
    );
    assert_eq!(
        control_flow(&db, may_fall_through).expect("fall-through flow"),
        Some(beskid_queries::ControlFlow { may_fall_through: true })
    );
}

#[test]
fn stale_generation_never_observes_semantic_facts() {
    let (mut db, project, unit, generation, index) = setup("i32 Main() { return 0; }");
    let current = key(unit, generation, &index, NodeKind::Literal, 0);
    assert_eq!(node_type(&db, current).expect("current type"), Some(beskid_queries::SemanticTypeId::I32));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { return 1; }".to_string(),
    )
    .expect("registered syntax edit");
    assert_eq!(node_type(&db, current).expect("stale fact"), None);
}

#[test]
fn local_resolution_never_guesses_from_positions() {
    let source = r#"
i32 First() { let hidden = 1; return hidden; }
i32 Second() { return hidden; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let first_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let second_reference = key(unit, generation, &index, NodeKind::PathExpression, 1);

    assert!(resolved_local(&db, first_reference).expect("first local").is_some());
    assert_eq!(resolved_local(&db, second_reference).expect("out-of-scope local"), None);
}

#[test]
fn local_resolution_uses_generation_safe_declarations_and_lexical_shadowing() {
    let source = r#"i32 Main(i32 value) {
    let first = value;
    if true {
        let value = 2;
        let nested = value;
    }
    return value;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let value_offsets = source.match_indices("value").map(|(offset, _)| offset).collect::<Vec<_>>();
    assert_eq!(value_offsets.len(), 5);

    let parameter = key_at_start(unit, generation, &index, NodeKind::Identifier, value_offsets[0]);
    let inner_declaration = key_at_start(unit, generation, &index, NodeKind::Identifier, value_offsets[2]);
    let parameter_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, value_offsets[1]);
    let inner_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, value_offsets[3]);
    let outer_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, value_offsets[4]);

    assert_eq!(
        resolved_local(&db, parameter_reference).expect("parameter reference"),
        Some(beskid_queries::ResolvedLocal { declaration: parameter })
    );
    assert_eq!(
        resolved_local(&db, inner_reference).expect("shadowed reference"),
        Some(beskid_queries::ResolvedLocal { declaration: inner_declaration })
    );
    assert_eq!(
        resolved_local(&db, outer_reference).expect("outer reference"),
        Some(beskid_queries::ResolvedLocal { declaration: parameter })
    );
}

#[test]
fn local_resolution_covers_lambda_for_and_match_bindings() {
    for source in [
        "i32 Main() { let apply = (i32 value) => value; return apply(1); }",
        "unit Main() { for item in [1] { let copy = item; } }",
        "enum Choice { Some(i32 value), None } i32 Main() { Choice choice = Choice::Some(1); return match choice { Choice::Some(bound) => bound, Choice::None => 0, }; }",
    ] {
        let (db, _project, unit, generation, index) = setup(source);
        let binding_name = if source.contains("value) =>") {
            "value"
        } else if source.contains("for item") {
            "item"
        } else {
            "bound"
        };
        let offsets = source.match_indices(binding_name).map(|(offset, _)| offset).collect::<Vec<_>>();
        assert_eq!(offsets.len(), 2, "{binding_name} occurrences in {source}");
        let declaration = key_at_start(unit, generation, &index, NodeKind::Identifier, offsets[0]);
        let reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, offsets[1]);
        assert_eq!(
            resolved_local(&db, reference).expect("binding reference"),
            Some(beskid_queries::ResolvedLocal { declaration }),
            "binding {binding_name} in {source}"
        );
    }
}

#[test]
fn local_declaration_is_not_visible_in_its_own_initializer() {
    let source = "i32 Main() { let value = value; return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let offsets = source.match_indices("value").map(|(offset, _)| offset).collect::<Vec<_>>();
    let initializer_reference = key_at_start(unit, generation, &index, NodeKind::PathExpression, offsets[1]);
    assert_eq!(resolved_local(&db, initializer_reference).expect("initializer local"), None);
}

#[test]
fn local_slots_are_stable_within_function_and_distinct_for_lambda_frames() {
    let source = r#"i32 Main(i32 value) {
    let outer = value;
    if true { let outer = 1; }
    let apply = (i32 inner) => inner;
    return outer;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let lambda_owner = key(unit, generation, &index, NodeKind::LambdaExpression, 0);
    let value_offsets = source.match_indices("value").map(|(offset, _)| offset).collect::<Vec<_>>();
    let outer_offsets = source.match_indices("outer").map(|(offset, _)| offset).collect::<Vec<_>>();
    let declarations = [
        key_at_start(unit, generation, &index, NodeKind::Identifier, value_offsets[0]),
        key_at_start(unit, generation, &index, NodeKind::Identifier, outer_offsets[0]),
        key_at_start(unit, generation, &index, NodeKind::Identifier, outer_offsets[1]),
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("apply").expect("apply declaration")),
    ];
    for (slot_index, declaration) in declarations.into_iter().enumerate() {
        assert_eq!(
            local_slot(&db, declaration).expect("function local slot"),
            Some(LocalSlot { owner, index: u32::try_from(slot_index).expect("slot index") })
        );
    }

    let inner =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("inner").expect("lambda parameter"));
    assert_eq!(local_slot(&db, inner).expect("lambda local slot"), Some(LocalSlot { owner: lambda_owner, index: 0 }));
    let function_name =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("Main").expect("function name"));
    assert_eq!(local_slot(&db, function_name).expect("ordinary name"), None);
}
