use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex, SyntaxSnapshot};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, OperatorFact, ProjectSession, SemanticError, SourceUnitId,
    SyntaxGenerationId, call_lowering, cast_intents, child_nodes, control_flow, direct_callees,
    item_body, item_signature, literal_fact, node_kind, node_span, node_type, operator_fact,
    reachable_items, resolved_item, resolved_local, runtime_intrinsic,
};

fn assert_unavailable<T>(result: Result<Option<T>, SemanticError>) {
    let error = match result {
        Ok(_) => panic!("current unported semantic query must fail explicitly"),
        Err(error) => error,
    };
    assert!(error.is_unavailable(), "{error:?}");
}

fn setup(
    source: &str,
) -> (
    BeskidDatabase,
    ProjectSession,
    SourceUnitId,
    SyntaxGenerationId,
    SyntaxIndex,
) {
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Main.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let generation = SyntaxGenerationId(3);
    let expanded = expand_program(
        parse_program(source).expect("parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let index = SyntaxIndex::from_program(&expanded, generation);
    db.ensure_file_text(unit.path(&db).clone(), source.to_string());
    db.ensure_syntax_unit(project, unit, generation)
        .expect("expanded syntax registration");
    (db, project, unit, generation, index)
}

#[test]
fn warm_point_query_uses_registered_expanded_syntax_without_reparse() {
    let (mut db, _project, unit, generation, index) = setup("i32 Main() { return 7; }");
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    assert!(literal_fact(&db, literal).expect("cold literal").is_some());
    assert_eq!(
        node_type(&db, literal).expect("cold type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );

    db.ensure_file_text(
        unit.path(&db).clone(),
        "this is deliberately invalid Beskid source".to_string(),
    );
    assert!(literal_fact(&db, literal).expect("warm literal").is_some());
    assert_eq!(
        node_type(&db, literal).expect("warm type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );
    assert_eq!(db.syntax_authority_counts(), (1, 1));
}

fn key(
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: &SyntaxIndex,
    kind: NodeKind,
    occurrence: usize,
) -> AstNodeKey {
    AstNodeKey {
        unit,
        generation,
        node: index
            .ids_of_kind(kind)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("missing {kind:?} occurrence {occurrence}")),
    }
}

fn key_at_start(
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: &SyntaxIndex,
    kind: NodeKind,
    start: usize,
) -> AstNodeKey {
    AstNodeKey {
        unit,
        generation,
        node: index
            .metadata()
            .iter()
            .find(|metadata| {
                metadata.kind == kind && metadata.span.is_some_and(|span| span.start == start)
            })
            .unwrap_or_else(|| panic!("missing {kind:?} at byte {start}"))
            .id,
    }
}

#[test]
fn structural_facts_survive_while_unported_semantics_are_unavailable() {
    let source = r#"
i32 Helper(i64 value) { return 1; }
i32 Main() {
    let local = 2;
    Helper(local);
    return local;
}
"#;
    let (db, _project, unit, generation, index) = setup(source);

    let helper = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let local_reference = key(unit, generation, &index, NodeKind::PathExpression, 2);
    let integer = key(unit, generation, &index, NodeKind::Literal, 0);
    let item_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(
        node_kind(&db, main).expect("kind"),
        Some(NodeKind::FunctionDefinition)
    );
    assert!(node_span(&db, main).expect("span").is_some());
    assert!(
        child_nodes(&db, main)
            .expect("children")
            .unwrap()
            .iter()
            .all(|child| child.unit == unit && child.generation == generation)
    );
    assert!(literal_fact(&db, integer).expect("literal").is_some());
    assert!(item_body(&db, main).expect("body").is_some());

    assert_eq!(
        node_type(&db, integer).expect("integer type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );
    assert_eq!(
        item_signature(&db, helper).expect("helper signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::I64].into(),
            result: beskid_queries::SemanticTypeId::I32,
        })
    );
    assert_eq!(
        item_signature(&db, main).expect("main signature"),
        Some(beskid_queries::ItemSignature {
            parameters: Arc::from([]),
            result: beskid_queries::SemanticTypeId::I32,
        })
    );
    assert_eq!(
        control_flow(&db, main).expect("control flow"),
        Some(beskid_queries::ControlFlow {
            may_fall_through: false,
        })
    );
    assert!(
        resolved_local(&db, local_reference)
            .expect("local resolution")
            .is_some()
    );
    assert_unavailable(resolved_item(&db, item_reference));
    assert_unavailable(call_lowering(&db, call));
    assert_unavailable(cast_intents(&db, call));
    assert_unavailable(direct_callees(&db, main));
    assert_unavailable(reachable_items(&db, main, main));
    assert_unavailable(runtime_intrinsic(&db, call));
}

#[test]
fn node_type_derives_primitive_literals_and_annotated_local_references() {
    let source = r#"unit Main(i64 input) {
    i64 local = input;
    let flag = true;
    let ratio = 1.5;
    let text = "text";
    let letter = 'x';
    let byte = 1_u8;
}"#;
    let (db, _project, unit, generation, index) = setup(source);
    let input_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert_eq!(
        node_type(&db, input_reference).expect("input type"),
        Some(beskid_queries::SemanticTypeId::I64)
    );
    let expected = [
        beskid_queries::SemanticTypeId::BOOL,
        beskid_queries::SemanticTypeId::F64,
        beskid_queries::SemanticTypeId::STRING,
        beskid_queries::SemanticTypeId::CHAR,
        beskid_queries::SemanticTypeId::U8,
    ];
    for (occurrence, expected) in expected.into_iter().enumerate() {
        let literal = key(unit, generation, &index, NodeKind::Literal, occurrence);
        assert_eq!(
            node_type(&db, literal).expect("literal type"),
            Some(expected)
        );
    }
}

#[test]
fn node_type_does_not_guess_complex_local_types() {
    let source = "Value Identity(Value input) { return input; }";
    let (db, _project, unit, generation, index) = setup(source);
    let input = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert_unavailable(node_type(&db, input));
}

#[test]
fn cast_intents_use_exact_typed_let_constraints_and_local_types() {
    let source = r#"unit Main() {
    i64 widenedLiteral = 1;
    i32 source = 2;
    i64 widenedLocal = source;
}"#;
    let (mut db, project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::Literal, 0);
    let source_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    let expected = Arc::from([beskid_queries::CastIntent {
        from: beskid_queries::SemanticTypeId::I32,
        to: beskid_queries::SemanticTypeId::I64,
    }]);
    assert_eq!(
        cast_intents(&db, literal).expect("literal cast"),
        Some(Arc::clone(&expected))
    );
    assert_eq!(
        cast_intents(&db, source_reference).expect("local cast"),
        Some(expected)
    );

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "unit Main() { i64 replacement = 1_i64; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(cast_intents(&db, literal).expect("stale cast"), None);
}

#[test]
fn item_signatures_cover_primitive_functions_methods_and_contracts() {
    let source = r#"
i64 Convert(i32 value, bool checked) { return value; }
type Counter { i64 value }
impl Counter { bool IsPositive(u8 threshold) { return true; } }
contract Converter { string Format(char value); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let contract = key(
        unit,
        generation,
        &index,
        NodeKind::ContractMethodSignature,
        0,
    );

    assert_eq!(
        item_signature(&db, function).expect("function signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [
                beskid_queries::SemanticTypeId::I32,
                beskid_queries::SemanticTypeId::BOOL,
            ]
            .into(),
            result: beskid_queries::SemanticTypeId::I64,
        })
    );
    assert_eq!(
        item_signature(&db, method).expect("method signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::U8].into(),
            result: beskid_queries::SemanticTypeId::BOOL,
        })
    );
    assert_eq!(
        item_signature(&db, contract).expect("contract signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::CHAR].into(),
            result: beskid_queries::SemanticTypeId::STRING,
        })
    );
}

#[test]
fn item_signature_does_not_guess_complex_type_identity() {
    let source = "Value Identity(Value value) { return value; }";
    let (db, _project, unit, generation, index) = setup(source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    assert_unavailable(item_signature(&db, function));
}

#[test]
fn call_lowering_classifies_immediate_lambda_without_name_resolution() {
    let source = "i64 Main() { return ((i64 value) => value)(1); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(
        call_lowering(&db, call).expect("lambda call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    );
}

#[test]
fn call_lowering_does_not_guess_named_targets() {
    let source = r#"
i64 Helper() { return 1; }
i64 Main() { return Helper(); }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let named_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert_unavailable(call_lowering(&db, named_call));
}

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
        Some(beskid_queries::ControlFlow {
            may_fall_through: false,
        })
    );
    assert_eq!(
        control_flow(&db, may_fall_through).expect("fall-through flow"),
        Some(beskid_queries::ControlFlow {
            may_fall_through: true,
        })
    );
}

#[test]
fn stale_generation_never_observes_semantic_facts() {
    let (mut db, project, unit, generation, index) = setup("i32 Main() { return 0; }");
    let current = key(unit, generation, &index, NodeKind::Literal, 0);
    assert_eq!(
        node_type(&db, current).expect("current type"),
        Some(beskid_queries::SemanticTypeId::I32)
    );

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

    assert!(
        resolved_local(&db, first_reference)
            .expect("first local")
            .is_some()
    );
    assert_eq!(
        resolved_local(&db, second_reference).expect("out-of-scope local"),
        None
    );
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
    let value_offsets = source
        .match_indices("value")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    assert_eq!(value_offsets.len(), 5);

    let parameter = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        value_offsets[0],
    );
    let inner_declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        value_offsets[2],
    );
    let parameter_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        value_offsets[1],
    );
    let inner_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        value_offsets[3],
    );
    let outer_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        value_offsets[4],
    );

    assert_eq!(
        resolved_local(&db, parameter_reference).expect("parameter reference"),
        Some(beskid_queries::ResolvedLocal {
            declaration: parameter,
        })
    );
    assert_eq!(
        resolved_local(&db, inner_reference).expect("shadowed reference"),
        Some(beskid_queries::ResolvedLocal {
            declaration: inner_declaration,
        })
    );
    assert_eq!(
        resolved_local(&db, outer_reference).expect("outer reference"),
        Some(beskid_queries::ResolvedLocal {
            declaration: parameter,
        })
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
        let offsets = source
            .match_indices(binding_name)
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        assert_eq!(offsets.len(), 2, "{binding_name} occurrences in {source}");
        let declaration = key_at_start(unit, generation, &index, NodeKind::Identifier, offsets[0]);
        let reference = key_at_start(
            unit,
            generation,
            &index,
            NodeKind::PathExpression,
            offsets[1],
        );
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
    let offsets = source
        .match_indices("value")
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    let initializer_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        offsets[1],
    );
    assert_eq!(
        resolved_local(&db, initializer_reference).expect("initializer local"),
        None
    );
}

#[test]
fn stale_generation_cannot_reuse_a_local_slot_identity() {
    let source = "i32 Main() { let value = 1; return value; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert!(
        resolved_local(&db, reference)
            .expect("current local")
            .is_some()
    );

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { let other = 1; return other; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(resolved_local(&db, reference).expect("stale local"), None);
}

#[test]
fn operator_facts_cover_expression_selection() {
    let source = "bool Main() { let value = 1 + 2; return !(value == 3); }";
    let (db, _project, unit, generation, index) = setup(source);
    let add = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let equals = key(unit, generation, &index, NodeKind::BinaryExpression, 1);
    let not = key(unit, generation, &index, NodeKind::UnaryExpression, 0);

    assert_eq!(
        operator_fact(&db, add).expect("operator"),
        Some(OperatorFact::Add)
    );
    assert_eq!(
        operator_fact(&db, equals).expect("operator"),
        Some(OperatorFact::Eq)
    );
    assert_eq!(
        operator_fact(&db, not).expect("operator"),
        Some(OperatorFact::Not)
    );
}

#[test]
fn item_body_is_the_exact_function_and_method_body_child() {
    let function_source = "i32 Main() { return 0; }";
    let (function_db, _project, unit, generation, index) = setup(function_source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let function_program = expand_program(
        parse_program(function_source).expect("function parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let function_snapshot = SyntaxSnapshot::from_program(&function_program, generation.0);
    let function_node = function_snapshot
        .node_at(function.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .expect("function definition");
    let expected_function_body = function_snapshot
        .id_of(DynNodeRef::from(&function_node.body))
        .expect("exact function body");
    assert_eq!(
        item_body(&function_db, function).expect("function body"),
        Some(AstNodeKey {
            node: beskid_analysis::syntax::AstNodeId(expected_function_body),
            ..function
        })
    );

    let method_source = "type Value { i32 raw } impl Value { i32 Get() { return this.raw; } }";
    let (method_db, _project, unit, generation, index) = setup(method_source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let method_program = expand_program(
        parse_program(method_source).expect("method parse"),
        DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
    );
    let method_snapshot = SyntaxSnapshot::from_program(&method_program, generation.0);
    let method_node = method_snapshot
        .node_at(method.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
        .expect("method definition");
    let expected_method_body = method_snapshot
        .id_of(DynNodeRef::from(&method_node.body))
        .expect("exact method body");
    assert_eq!(
        item_body(&method_db, method).expect("method body"),
        Some(AstNodeKey {
            node: beskid_analysis::syntax::AstNodeId(expected_method_body),
            ..method
        })
    );
}
