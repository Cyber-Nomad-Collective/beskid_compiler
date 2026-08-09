use super::support::{assert_unavailable, key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    SemanticTypeId, call_lowering, cast_intents, child_nodes, control_flow, direct_callees, enum_match, item_body,
    item_signature, literal_fact, node_kind, node_span, node_type, reachable_items, resolved_item, resolved_local,
    runtime_intrinsic,
};
use std::sync::Arc;

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
    let program = key(unit, generation, &index, NodeKind::Program, 0);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let local_reference = key(unit, generation, &index, NodeKind::PathExpression, 2);
    let integer = key(unit, generation, &index, NodeKind::Literal, 0);
    let item_reference = key(unit, generation, &index, NodeKind::PathExpression, 0);

    assert_eq!(node_kind(&db, main).expect("kind"), Some(NodeKind::FunctionDefinition));
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

    assert_eq!(node_type(&db, integer).expect("integer type"), Some(beskid_queries::SemanticTypeId::I32));
    assert_eq!(
        item_signature(&db, helper).expect("helper signature"),
        Some(beskid_queries::ItemSignature {
            parameters: [beskid_queries::SemanticTypeId::I64].into(),
            result: beskid_queries::SemanticTypeId::I32,
        })
    );
    assert_eq!(
        item_signature(&db, main).expect("main signature"),
        Some(beskid_queries::ItemSignature { parameters: Arc::from([]), result: beskid_queries::SemanticTypeId::I32 })
    );
    assert_eq!(
        control_flow(&db, main).expect("control flow"),
        Some(beskid_queries::ControlFlow { may_fall_through: false })
    );
    assert!(resolved_local(&db, local_reference).expect("local resolution").is_some());
    assert_eq!(
        resolved_item(&db, item_reference).expect("item resolution"),
        Some(beskid_queries::ResolvedItem { declaration: helper })
    );
    assert_eq!(call_lowering(&db, call).expect("call lowering"), Some(beskid_queries::CallLowering::Direct(helper)));
    assert_unavailable(cast_intents(&db, call));
    assert_eq!(direct_callees(&db, main).expect("direct callees"), Some(Arc::from([helper])));
    assert_eq!(reachable_items(&db, program, main).expect("reachable items"), Some(Arc::from([main, helper])));
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
    assert_eq!(node_type(&db, input_reference).expect("input type"), Some(beskid_queries::SemanticTypeId::I64));
    let expected = [
        beskid_queries::SemanticTypeId::BOOL,
        beskid_queries::SemanticTypeId::F64,
        beskid_queries::SemanticTypeId::STRING,
        beskid_queries::SemanticTypeId::CHAR,
        beskid_queries::SemanticTypeId::U8,
    ];
    for (occurrence, expected) in expected.into_iter().enumerate() {
        let literal = key(unit, generation, &index, NodeKind::Literal, occurrence);
        assert_eq!(node_type(&db, literal).expect("literal type"), Some(expected));
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
fn node_type_uses_the_exact_scalar_enum_payload_binding_shape() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(value) => value, Result::Error(error) => error, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let value = key(unit, generation, &index, NodeKind::PathExpression, 1);
    let error = key(unit, generation, &index, NodeKind::PathExpression, 2);

    assert_eq!(node_type(&db, value).expect("Ok binding type"), Some(SemanticTypeId::I64));
    assert_eq!(node_type(&db, error).expect("Error binding type"), Some(SemanticTypeId::I64));
}

#[test]
fn node_type_uses_the_exact_nominal_enum_payload_binding_shape() {
    let source = "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } unit Main(Descriptor descriptor) { match descriptor { Descriptor::Standard(stream) => { stream; }, Descriptor::Raw(_) => {}, }; return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let stream = key(unit, generation, &index, NodeKind::PathExpression, 1);

    assert_eq!(node_type(&db, stream).expect("StandardStream binding type"), Some(SemanticTypeId::POINTER));
}

#[test]
fn enum_match_uses_the_exact_nominal_pattern_binding_layout() {
    let source = "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } i64 Main(Descriptor descriptor) { return match descriptor { Descriptor::Standard(stream) => match stream { StandardStream::Stdin => 0_i64, StandardStream::Stdout => 1_i64, StandardStream::Stderr => 2_i64, }, Descriptor::Raw(fd) => fd, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let inner_match = key(unit, generation, &index, NodeKind::MatchExpression, 1);

    assert!(
        enum_match(&db, inner_match).expect("inner match query").is_some(),
        "a direct nominal payload binding must supply the inner enum layout"
    );
}

#[test]
fn node_type_composes_enum_match_results_from_binding_aware_arm_nodes() {
    let source = "enum StandardStream { Stdin, Stdout, Stderr } enum Descriptor { Standard(StandardStream stream), Raw(i64 fd) } i64 Main(Descriptor descriptor) { return match descriptor { Descriptor::Standard(stream) => match stream { StandardStream::Stdin => 0_i64, StandardStream::Stdout => 1_i64, StandardStream::Stderr => 2_i64, }, Descriptor::Raw(fd) => fd, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let outer_match = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_eq!(node_type(&db, outer_match).expect("outer match type"), Some(SemanticTypeId::I64));
}

#[test]
fn node_type_rejects_enum_match_results_with_mixed_arm_types() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } unit Main(Result result) { match result { Result::Ok(value) => value, Result::Error(error) => true, }; return; }";
    let (db, _project, unit, generation, index) = setup(source);
    let outer_match = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_unavailable(node_type(&db, outer_match));
}

#[test]
fn node_type_uses_an_exact_direct_call_abi_result() {
    let source = "i64 Fd() { return 0_i64; } i64 Main() { return Fd(); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);

    assert_eq!(node_type(&db, call).expect("direct call type"), Some(SemanticTypeId::I64));
}

#[test]
fn literal_enum_payload_pattern_remains_unavailable_to_type_queries() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } i64 Main(Result result) { return match result { Result::Ok(7_i64) => 1_i64, Result::Error(_) => 0_i64, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);

    assert_unavailable(enum_match(&db, expression));
}
