use super::support::{assert_unavailable, key, key_at_start, setup};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxSnapshot};
use beskid_queries::{
    AstNodeKey, ItemSignature, LocalSlot, MutableLocalAssignment, OperatorFact, SemanticTypeId, SyntaxGenerationId,
    abi_type, contextual_integer_literal_abi_type, for_iterator_fact, item_body, item_signature, local_slot,
    mutable_local_assignment, node_type, operator_fact, resolved_local,
};
use std::sync::Arc;

#[test]
fn syntax_only_signatures_preserve_runtime_pointer_and_never_primitives() {
    let source = "pointer Echo(pointer value) { return value; } never Stop() { while true {} }";
    let (db, _project, unit, generation, index) = setup(source);
    let echo = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let stop = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);

    assert_eq!(
        item_signature(&db, echo).expect("pointer signature"),
        Some(ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::POINTER })
    );
    assert_eq!(
        item_signature(&db, stop).expect("never signature"),
        Some(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::NEVER })
    );
}

#[test]
fn local_slots_cover_methods_for_iterators_and_match_bindings() {
    let method_source = r#"type Value { i32 raw }
impl Value { i32 Sum(i32 first) { let local = first; return local; } }"#;
    let (db, _project, unit, generation, index) = setup(method_source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    for (name, expected_index) in [("first", 0), ("local", 1)] {
        let declaration = key_at_start(
            unit,
            generation,
            &index,
            NodeKind::Identifier,
            method_source.find(name).expect("method declaration"),
        );
        assert_eq!(
            local_slot(&db, declaration).expect("method local slot"),
            Some(LocalSlot { owner: method, index: expected_index })
        );
    }

    for (source, declarations) in [
        ("unit Main() { for item in [1] { let copy = item; } }", [("item", 0), ("copy", 1)]),
        (
            "enum Choice { Some(i32 value), None } i32 Main() { Choice choice = Choice::Some(1); return match choice { Choice::Some(bound) => bound, Choice::None => 0, }; }",
            [("choice", 0), ("bound", 1)],
        ),
    ] {
        let (db, _project, unit, generation, index) = setup(source);
        let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
        for (name, expected_index) in declarations {
            let declaration = key_at_start(
                unit,
                generation,
                &index,
                NodeKind::Identifier,
                source.find(name).expect("binding declaration"),
            );
            assert_eq!(
                local_slot(&db, declaration).expect("binding local slot"),
                Some(LocalSlot { owner, index: expected_index }),
                "binding {name} in {source}"
            );
        }
    }
}

#[test]
fn for_iterator_fact_proves_range_element_type_and_shadowing() {
    use beskid_queries::ForIteratorFact;

    let source = "i32 Main() { let value = 1_i64; for value in range(1, 4) { let copy = value; } return 0; }";
    let (db, _project, unit, generation, index) = setup(source);
    let for_stmt = key(unit, generation, &index, NodeKind::ForStatement, 0);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("for value").expect("for header") + "for ".len(),
    );
    assert_eq!(
        for_iterator_fact(&db, for_stmt).expect("for iterator fact"),
        Some(ForIteratorFact { declaration, element_type: SemanticTypeId::I32 })
    );
    assert_eq!(node_type(&db, declaration).expect("iterator declaration type"), Some(SemanticTypeId::I32));
    let body_reference = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        source.find("= value").expect("body use") + "= ".len(),
    );
    assert_eq!(
        resolved_local(&db, body_reference).expect("iterator reference").map(|resolved| resolved.declaration),
        Some(declaration)
    );
    assert_eq!(node_type(&db, body_reference).expect("shadowed iterator type"), Some(SemanticTypeId::I32));

    let nested = "i32 Main() { for outer in range(1, 3) { for outer in range(10_i64, 12_i64) { let inner = outer; } } return 0; }";
    let (db, _project, unit, generation, index) = setup(nested);
    let outer_for = key(unit, generation, &index, NodeKind::ForStatement, 0);
    let inner_for = key(unit, generation, &index, NodeKind::ForStatement, 1);
    assert_eq!(
        for_iterator_fact(&db, outer_for).expect("outer for").map(|fact| fact.element_type),
        Some(SemanticTypeId::I32)
    );
    assert_eq!(
        for_iterator_fact(&db, inner_for).expect("inner for").map(|fact| fact.element_type),
        Some(SemanticTypeId::I64)
    );
    let inner_use = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::PathExpression,
        nested.find("= outer").expect("inner use") + "= ".len(),
    );
    assert_eq!(node_type(&db, inner_use).expect("nested shadow type"), Some(SemanticTypeId::I64));
}

#[test]
fn stale_generation_cannot_reuse_for_iterator_fact() {
    let source = "i32 Main() { for value in range(1, 4) { let copy = value; } return 0; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let for_stmt = key(unit, generation, &index, NodeKind::ForStatement, 0);
    assert!(for_iterator_fact(&db, for_stmt).expect("current for iterator").is_some());
    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { for other in range(1, 4) { let copy = other; } return 0; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(for_iterator_fact(&db, for_stmt).expect("stale for iterator"), None);
}

#[test]
fn for_iterator_fact_rejects_non_range_iterables() {
    let source = "unit Main() { for item in [1] { let copy = item; } }";
    let (db, _project, unit, generation, index) = setup(source);
    let for_stmt = key(unit, generation, &index, NodeKind::ForStatement, 0);
    assert_unavailable(for_iterator_fact(&db, for_stmt));
}

#[test]
fn stale_generation_cannot_reuse_a_local_slot_identity() {
    let source = "i32 Main() { let value = 1; return value; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let declaration =
        key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("value").expect("local declaration"));
    let owner = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let reference = key(unit, generation, &index, NodeKind::PathExpression, 0);
    assert!(resolved_local(&db, reference).expect("current local").is_some());
    assert_eq!(local_slot(&db, declaration).expect("current local slot"), Some(LocalSlot { owner, index: 0 }));

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { let other = 1; return other; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(resolved_local(&db, reference).expect("stale local"), None);
    assert_eq!(local_slot(&db, declaration).expect("stale slot"), None);
}

#[test]
fn mutable_local_assignment_requires_a_current_mutable_lexical_declaration() {
    let source = "i32 Main() { mut i32 total = 0; total = total + 1; return total; }";
    let (mut db, project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);
    let declaration = key_at_start(
        unit,
        generation,
        &index,
        NodeKind::Identifier,
        source.find("total").expect("mutable declaration"),
    );
    assert_eq!(
        mutable_local_assignment(&db, assignment).expect("mutable assignment fact"),
        Some(MutableLocalAssignment {
            declaration,
            slot: local_slot(&db, declaration)
                .expect("mutable declaration slot")
                .expect("mutable declaration slot fact"),
        })
    );

    db.update_syntax_source(
        project,
        unit,
        SyntaxGenerationId(generation.0 + 1),
        "i32 Main() { i32 total = 0; total = total + 1; return total; }".to_string(),
    )
    .expect("syntax update");
    assert_eq!(
        mutable_local_assignment(&db, assignment).expect("stale assignment fact"),
        None,
        "a stale assignment key cannot retain write authority"
    );
}

#[test]
fn immutable_local_assignment_is_an_explicit_unavailable_syntax_fact() {
    let source = "i32 Main() { i32 total = 0; total = total + 1; return total; }";
    let (db, _project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);
    assert_unavailable(mutable_local_assignment(&db, assignment));
}

#[test]
fn contextual_integer_literal_abi_type_contextualizes_declared_struct_fields() {
    let source = "type Cursor { i64 pos } Cursor Main() { return Cursor { pos: 0 }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_eq!(
        contextual_integer_literal_abi_type(&db, literal).expect("typed struct field literal"),
        Some(SemanticTypeId::I64)
    );
}

#[test]
fn array_annotations_have_pointer_abi_facts_for_locals_and_bindings() {
    let source = "unit Main() { u8[] bytes = __array_new(1, 0); }";
    let (db, _project, unit, generation, index) = setup(source);
    let local = key(unit, generation, &index, NodeKind::LetStatement, 0);
    let binding = key(unit, generation, &index, NodeKind::Identifier, 1);

    assert_eq!(abi_type(&db, local).expect("array local ABI"), Some(SemanticTypeId::POINTER));
    assert_eq!(abi_type(&db, binding).expect("array binding ABI"), Some(SemanticTypeId::POINTER));
}

#[test]
fn declared_array_index_assignment_has_its_proven_element_abi() {
    let source = "string[] Store(string[] values, string value) { values[0] = value; return values; }";
    let (db, _project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);

    assert_eq!(
        abi_type(&db, assignment).expect("declared array assignment ABI"),
        Some(SemanticTypeId::STRING),
        "only the declared string[] target may supply the assignment result ABI"
    );
}

#[test]
fn declared_array_compound_index_assignment_has_no_element_abi_fact() {
    let source = "i32[] Store(i32[] values, i32 value) { values[0] += value; return values; }";
    let (db, _project, unit, generation, index) = setup(source);
    let assignment = key(unit, generation, &index, NodeKind::AssignExpression, 0);

    assert_unavailable(abi_type(&db, assignment));
}

#[test]
fn contextual_integer_literal_abi_type_contextualizes_only_bare_integer_literals_at_exact_declared_boundaries() {
    let source = "i64 Main(mut i64 start) { i64 offset = 0; start = 1; return start + offset; }";
    let (db, _project, unit, generation, index) = setup(source);
    let first = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    let second = key(unit, generation, &index, NodeKind::LiteralExpression, 1);

    assert_eq!(contextual_integer_literal_abi_type(&db, first).expect("typed let literal"), Some(SemanticTypeId::I64));
    assert_eq!(
        contextual_integer_literal_abi_type(&db, second).expect("typed assignment literal"),
        Some(SemanticTypeId::I64)
    );

    let struct_field = "type Cursor { i64 pos } Cursor Main() { return Cursor { pos: 0 }; }";
    let (db, _project, unit, generation, index) = setup(struct_field);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_eq!(
        contextual_integer_literal_abi_type(&db, literal).expect("typed struct field literal"),
        Some(SemanticTypeId::I64)
    );

    let inferred = "i32 Main() { let value = 0; return value; }";
    let (db, _project, unit, generation, index) = setup(inferred);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_unavailable(contextual_integer_literal_abi_type(&db, literal));

    let explicitly_suffixed = "i64 Main() { i64 value = 0_i32; return value; }";
    let (db, _project, unit, generation, index) = setup(explicitly_suffixed);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_unavailable(contextual_integer_literal_abi_type(&db, literal));
}

#[test]
fn contextual_integer_literal_abi_type_return_boundary_rejects_literals_nested_in_return_calls() {
    let direct_return = "i32 Main() { return 8; }";
    let (db, _project, unit, generation, index) = setup(direct_return);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_eq!(
        contextual_integer_literal_abi_type(&db, literal).expect("typed direct return literal"),
        Some(SemanticTypeId::I32)
    );

    let negated_return = "i64 Main() { return -1; }";
    let (db, _project, unit, generation, index) = setup(negated_return);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_eq!(
        contextual_integer_literal_abi_type(&db, literal).expect("typed negated return literal"),
        Some(SemanticTypeId::I64)
    );

    let nested_in_return_call = "i32 Main(pointer state) { return i32(pointer_add(state, 8)); }";
    let (db, _project, unit, generation, index) = setup(nested_in_return_call);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_unavailable(contextual_integer_literal_abi_type(&db, literal));

    let nested_in_return_call_same_width = "pointer Main(pointer state) { return pointer_add(state, 8); }";
    let (db, _project, unit, generation, index) = setup(nested_in_return_call_same_width);
    let literal = key(unit, generation, &index, NodeKind::LiteralExpression, 0);
    assert_unavailable(contextual_integer_literal_abi_type(&db, literal));
}

#[test]
fn operator_facts_cover_expression_selection() {
    let source = "bool Main() { let value = 1 + 2; return !(value == 3); }";
    let (db, _project, unit, generation, index) = setup(source);
    let add = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let equals = key(unit, generation, &index, NodeKind::BinaryExpression, 1);
    let not = key(unit, generation, &index, NodeKind::UnaryExpression, 0);

    assert_eq!(operator_fact(&db, add).expect("operator"), Some(OperatorFact::Add));
    assert_eq!(operator_fact(&db, equals).expect("operator"), Some(OperatorFact::Eq));
    assert_eq!(operator_fact(&db, not).expect("operator"), Some(OperatorFact::Not));
}

#[test]
fn string_interpolation_desugar_uses_string_add_facts() {
    let source = r#"
string Prefix() { return "x"; }
string Main(string body) { return "${Prefix()}${body}!"; }
"#;
    let (db, _project, unit, generation, index) = setup(source);
    let outer = key(unit, generation, &index, NodeKind::BinaryExpression, 0);
    let inner = key(unit, generation, &index, NodeKind::BinaryExpression, 1);

    assert_eq!(operator_fact(&db, inner).expect("inner string add"), Some(OperatorFact::StringAdd));
    assert_eq!(operator_fact(&db, outer).expect("outer string add"), Some(OperatorFact::StringAdd));
    assert_eq!(abi_type(&db, inner).expect("inner abi"), Some(SemanticTypeId::STRING));
    assert_eq!(abi_type(&db, outer).expect("outer abi"), Some(SemanticTypeId::STRING));
    assert_eq!(node_type(&db, outer).expect("outer node type"), Some(SemanticTypeId::STRING));
}

#[test]
fn string_interpolation_numeric_operand_has_string_result_facts() {
    let source = r#"string Main(i64 value) { return "value=${value}"; }"#;
    let (db, _project, unit, generation, index) = setup(source);
    let interpolation = key(unit, generation, &index, NodeKind::BinaryExpression, 0);

    assert_eq!(operator_fact(&db, interpolation).expect("string add"), Some(OperatorFact::StringAdd));
    assert_eq!(abi_type(&db, interpolation).expect("interpolation abi"), Some(SemanticTypeId::STRING));
    assert_eq!(node_type(&db, interpolation).expect("interpolation node type"), Some(SemanticTypeId::STRING));
}

#[test]
fn item_body_is_the_exact_function_and_method_body_child() {
    let function_source = "i32 Main() { return 0; }";
    let (function_db, _project, unit, generation, index) = setup(function_source);
    let function = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let function_program =
        expand_program(parse_program(function_source).expect("function parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let function_snapshot = SyntaxSnapshot::from_program(&function_program, generation.0);
    let function_node = function_snapshot
        .node_at(function.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .expect("function definition");
    let expected_function_body =
        function_snapshot.id_of(DynNodeRef::from(&function_node.body)).expect("exact function body");
    assert_eq!(
        item_body(&function_db, function).expect("function body"),
        Some(AstNodeKey { node: beskid_analysis::syntax::AstNodeId(expected_function_body), ..function })
    );

    let method_source = "type Value { i32 raw } impl Value { i32 Get() { return this.raw; } }";
    let (method_db, _project, unit, generation, index) = setup(method_source);
    let method = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let method_program =
        expand_program(parse_program(method_source).expect("method parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let method_snapshot = SyntaxSnapshot::from_program(&method_program, generation.0);
    let method_node = method_snapshot
        .node_at(method.node.0)
        .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
        .expect("method definition");
    let expected_method_body = method_snapshot.id_of(DynNodeRef::from(&method_node.body)).expect("exact method body");
    assert_eq!(
        item_body(&method_db, method).expect("method body"),
        Some(AstNodeKey { node: beskid_analysis::syntax::AstNodeId(expected_method_body), ..method })
    );
}
