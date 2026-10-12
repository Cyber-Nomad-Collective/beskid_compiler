//! One bidirectional `expected_type` query supplies every contextual fact: the type a position
//! requires, derived from the typed parent context, forwarded through arms, elements, blocks and
//! bare-literal binary peers, and unavailable wherever no declaration or sibling proves it.
use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ExpectedType, ExpectedTypeContext, GenericSubstitution, ProjectSession, SemanticError,
    SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type, contextual_integer_literal_abi_type,
    empty_array_literal_element_abi_type, expected_type, expected_type_in_environment, generic_specialization_instance,
};

struct Fixture {
    db: BeskidDatabase,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: SyntaxIndex,
}

impl Fixture {
    fn new(source: &str) -> Self {
        let mut db = BeskidDatabase::default();
        let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/expected-type/src/Main.bd"));
        let project = ProjectSession::new(
            &db,
            PathBuf::from("/tmp/expected-type"),
            unit.path(&db).clone(),
            "App".to_string(),
            "lock".to_string(),
        );
        let generation = SyntaxGenerationId(5);
        let expanded = expand_program(parse_program(source).expect("parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
        let index = SyntaxIndex::from_program(&expanded, generation);
        db.ensure_file_text(unit.path(&db).clone(), source.to_string());
        db.ensure_syntax_unit(project, unit, generation).expect("expanded syntax registration");
        Self { db, unit, generation, index }
    }

    fn key(&self, kind: NodeKind, occurrence: usize) -> AstNodeKey {
        AstNodeKey {
            unit: self.unit,
            generation: self.generation,
            node: self
                .index
                .ids_of_kind(kind)
                .nth(occurrence)
                .unwrap_or_else(|| panic!("missing {kind:?} occurrence {occurrence}")),
        }
    }

    fn literal(&self, occurrence: usize) -> AstNodeKey {
        self.key(NodeKind::Literal, occurrence)
    }

    fn expected(&self, key: AstNodeKey) -> ExpectedType {
        expected_type(&self.db, key).expect("expected type query").expect("expected type fact")
    }

    fn integer_abi(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        contextual_integer_literal_abi_type(&self.db, key).expect("contextual integer query")
    }
}

fn assert_unavailable<T>(result: Result<Option<T>, SemanticError>) {
    match result {
        Ok(_) => panic!("a position without a typed context must fail closed"),
        Err(error) => assert!(error.is_unavailable(), "{error:?}"),
    }
}

#[test]
fn declared_local_supplies_its_annotation_and_an_inferred_local_supplies_nothing() {
    let fixture = Fixture::new("unit Main() { u8 value = 1; let inferred = 2; }");
    let declared = fixture.expected(fixture.literal(0));
    assert!(matches!(declared.context, ExpectedTypeContext::DeclaredLocal { statement } if statement == fixture.key(NodeKind::LetStatement, 0)));
    assert_eq!(declared.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(0)), Some(SemanticTypeId::U8));

    assert_unavailable(expected_type(&fixture.db, fixture.literal(1)));
    assert_unavailable(contextual_integer_literal_abi_type(&fixture.db, fixture.literal(1)));
}

#[test]
fn assignment_supplies_the_target_declaration_type() {
    let fixture = Fixture::new("unit Main(mut u16 count) { count = 7; }");
    let expected = fixture.expected(fixture.literal(0));
    assert!(matches!(expected.context, ExpectedTypeContext::Assignment { assignment } if assignment == fixture.key(NodeKind::AssignExpression, 0)));
    assert_eq!(expected.abi_type(), SemanticTypeId::U16);
    assert_eq!(fixture.integer_abi(fixture.literal(0)), Some(SemanticTypeId::U16));
}

#[test]
fn call_arguments_take_the_matching_callee_parameter_by_position() {
    let fixture = Fixture::new("unit Pair(u8 first, u16 second) { return; } unit Main() { Pair(1, 2); return; }");
    let call = fixture.key(NodeKind::CallExpression, 0);
    let first = fixture.expected(fixture.literal(0));
    let second = fixture.expected(fixture.literal(1));
    assert_eq!(first.context, ExpectedTypeContext::CallArgument { call, position: 0 });
    assert_eq!(second.context, ExpectedTypeContext::CallArgument { call, position: 1 });
    assert_eq!(first.abi_type(), SemanticTypeId::U8);
    assert_eq!(second.abi_type(), SemanticTypeId::U16);
    assert_eq!(fixture.integer_abi(fixture.literal(0)), Some(SemanticTypeId::U8));
    assert_eq!(fixture.integer_abi(fixture.literal(1)), Some(SemanticTypeId::U16));
}

#[test]
fn explicit_generic_call_arguments_substitute_the_callee_type_parameter() {
    let fixture = Fixture::new(
        "unit Append<T>(T[] values, T value) { return; } unit Main() { Append<u8>([], 1); Append([], 1_u8); return; }",
    );
    let explicit_array = fixture.key(NodeKind::ArrayLiteralExpression, 0);
    assert_eq!(fixture.expected(explicit_array).array_element_abi_type(), Some(SemanticTypeId::U8));
    assert_eq!(empty_array_literal_element_abi_type(&fixture.db, explicit_array), Ok(Some(SemanticTypeId::U8)));
    assert_eq!(fixture.integer_abi(fixture.literal(0)), Some(SemanticTypeId::U8));

    // Inference from the sibling argument is never consulted: it would be a cycle through
    // argument typing, and the array stays unavailable.
    let inferred_array = fixture.key(NodeKind::ArrayLiteralExpression, 1);
    assert_unavailable(expected_type(&fixture.db, inferred_array));
    assert_unavailable(empty_array_literal_element_abi_type(&fixture.db, inferred_array));
}

#[test]
fn a_trailing_bulk_argument_expects_the_element_and_a_numeric_conversion_supplies_nothing() {
    // Every trailing argument of a `bulk u8[]` callee is one `u8`; the callee receives the pack.
    let bulk = Fixture::new("unit Pack(bulk u8[] values) { return; } unit Main() { Pack(1, 2); return; }");
    let first = bulk.expected(bulk.literal(0));
    assert!(matches!(first.context, ExpectedTypeContext::CallArgument { position: 0, .. }), "{first:?}");
    assert_eq!(first.abi_type(), SemanticTypeId::U8);
    assert_eq!(bulk.integer_abi(bulk.literal(1)), Some(SemanticTypeId::U8));
    // A generic element is never inferred from the sibling arguments.
    let generic = Fixture::new("T First<T>(bulk T[] values) { return values[0]; } i64 Main() { return First(1, 2); }");
    assert_unavailable(expected_type(&generic.db, generic.literal(1)));

    let conversion = Fixture::new("u8 Main() { return u8(300); }");
    assert_unavailable(expected_type(&conversion.db, conversion.literal(0)));
    assert_unavailable(contextual_integer_literal_abi_type(&conversion.db, conversion.literal(0)));
}

#[test]
fn runtime_intrinsic_arguments_take_the_manifest_parameter_abi() {
    let fixture = Fixture::new("pointer Main(pointer state) { return pointer_add(state, 8); }");
    let expected = fixture.expected(fixture.literal(0));
    assert!(matches!(expected.context, ExpectedTypeContext::CallArgument { position: 1, .. }));
    assert_eq!(expected.abi_type(), SemanticTypeId::WORD);
    assert_eq!(fixture.integer_abi(fixture.literal(0)), Some(SemanticTypeId::WORD));
}

#[test]
fn enum_payloads_take_their_declared_slot_including_applied_generic_arguments() {
    let fixture = Fixture::new(
        "enum Pair { Both(u8 first, u16 second) } enum Holder<T> { Item(T value) } unit Main() { Pair::Both(1, 2); Holder<u16>::Item(3); return; }",
    );
    let pair = fixture.key(NodeKind::EnumConstructorExpression, 0);
    let first = fixture.expected(fixture.literal(0));
    let second = fixture.expected(fixture.literal(1));
    assert_eq!(first.context, ExpectedTypeContext::EnumPayload { constructor: pair, position: 0 });
    assert_eq!(second.context, ExpectedTypeContext::EnumPayload { constructor: pair, position: 1 });
    assert_eq!(first.abi_type(), SemanticTypeId::U8);
    assert_eq!(second.abi_type(), SemanticTypeId::U16);
    assert_eq!(fixture.expected(fixture.literal(2)).abi_type(), SemanticTypeId::U16);
    assert_eq!(fixture.integer_abi(fixture.literal(2)), Some(SemanticTypeId::U16));
}

#[test]
fn struct_fields_take_their_declared_type_including_applied_generic_arguments() {
    let fixture = Fixture::new(
        "type Point { u8 x, u16 y } type Applied<T> { T value } unit Main() { Point point = Point { x: 1, y: 2 }; Applied<i32> applied = Applied<i32> { value: 7 }; return; }",
    );
    let point = fixture.key(NodeKind::StructLiteralExpression, 0);
    let x = fixture.expected(fixture.literal(0));
    let y = fixture.expected(fixture.literal(1));
    assert_eq!(x.context, ExpectedTypeContext::StructField { literal: point, field: Arc::from("x") });
    assert_eq!(y.context, ExpectedTypeContext::StructField { literal: point, field: Arc::from("y") });
    assert_eq!(x.abi_type(), SemanticTypeId::U8);
    assert_eq!(y.abi_type(), SemanticTypeId::U16);
    assert_eq!(fixture.expected(fixture.literal(2)).abi_type(), SemanticTypeId::I32);
    assert_eq!(fixture.integer_abi(fixture.literal(2)), Some(SemanticTypeId::I32));
}

#[test]
fn return_values_take_the_declared_return_type() {
    let fixture = Fixture::new("u8 Main() { return 1; }");
    let expected = fixture.expected(fixture.literal(0));
    assert!(matches!(expected.context, ExpectedTypeContext::Return { item } if item == fixture.key(NodeKind::FunctionDefinition, 0)));
    assert_eq!(expected.abi_type(), SemanticTypeId::U8);

    let arrays = Fixture::new("u8[] Main() { return []; }");
    let array = arrays.key(NodeKind::ArrayLiteralExpression, 0);
    assert_eq!(empty_array_literal_element_abi_type(&arrays.db, array), Ok(Some(SemanticTypeId::U8)));
}

#[test]
fn array_elements_take_the_element_of_the_expected_array_in_declared_and_argument_positions() {
    let fixture = Fixture::new(
        "unit Append<T>(T[] values, T value) { return; } unit Main() { u8[] bytes = [1, 2]; Append<u16>([3], 4); return; }",
    );
    let bytes = fixture.key(NodeKind::ArrayLiteralExpression, 0);
    let first = fixture.expected(fixture.literal(0));
    assert_eq!(first.context, ExpectedTypeContext::ArrayElement { array: bytes });
    assert_eq!(first.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(1)), Some(SemanticTypeId::U8));

    // The element of an argument array forwards through the explicit generic parameter.
    let argument = fixture.key(NodeKind::ArrayLiteralExpression, 1);
    assert_eq!(fixture.expected(argument).array_element_abi_type(), Some(SemanticTypeId::U16));
    assert_eq!(fixture.integer_abi(fixture.literal(2)), Some(SemanticTypeId::U16));
    assert_eq!(fixture.integer_abi(fixture.literal(3)), Some(SemanticTypeId::U16));
}

#[test]
fn match_arms_forward_the_match_expectation() {
    let fixture = Fixture::new(
        "enum Flag { On, Off } u8 Main(Flag flag) { u8 value = match flag { Flag::On => 1, Flag::Off => 2, }; return value; }",
    );
    let expression = fixture.key(NodeKind::MatchExpression, 0);
    let first = fixture.expected(fixture.literal(0));
    assert_eq!(first.context, ExpectedTypeContext::MatchArm { expression });
    assert_eq!(first.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(1)), Some(SemanticTypeId::U8));
}

#[test]
fn block_expressions_and_discarded_statements_forward_nothing() {
    let fixture = Fixture::new(
        "enum Flag { On, Off } u8 Main(Flag flag) { u8 value = match flag { Flag::On => { 1; }, Flag::Off => { 2; 3; }, }; 4; return value; }",
    );
    // A value written inside a block has no direct typed context: `{ 1; }` is not `1`.
    for occurrence in 0..3 {
        let discarded = fixture.expected(fixture.literal(occurrence));
        assert!(matches!(discarded.context, ExpectedTypeContext::ExpressionStatement { .. }));
        assert_eq!(discarded.abi_type(), SemanticTypeId::UNIT);
        assert_unavailable(contextual_integer_literal_abi_type(&fixture.db, fixture.literal(occurrence)));
    }
    assert_eq!(fixture.expected(fixture.literal(3)).abi_type(), SemanticTypeId::UNIT);
}

#[test]
fn binary_operands_take_the_peer_representation_or_forward_the_expression_expectation() {
    let fixture = Fixture::new("u8 Main(u8 b) { u8 sum = 1 + 2; bool same = b == 3; return b - 4; }");
    let sum = fixture.key(NodeKind::BinaryExpression, 0);
    let forwarded = fixture.expected(fixture.literal(0));
    assert!(matches!(forwarded.context, ExpectedTypeContext::BinaryOperand { expression, .. } if expression == sum));
    assert_eq!(forwarded.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(1)), Some(SemanticTypeId::U8));
    assert_eq!(abi_type(&fixture.db, sum).expect("sum ABI"), Some(SemanticTypeId::U8));

    let compared = fixture.expected(fixture.literal(2));
    assert!(matches!(compared.context, ExpectedTypeContext::BinaryOperand { .. }));
    assert_eq!(compared.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(3)), Some(SemanticTypeId::U8));

    // Two bare literals compared against each other prove nothing about each other.
    let unrelated = Fixture::new("bool Main() { return 1 == 2; }");
    assert_unavailable(expected_type(&unrelated.db, unrelated.literal(0)));
}

#[test]
fn an_enclosing_specialization_interprets_declared_generic_types() {
    let fixture = Fixture::new("T[] Make<T>() { return []; }");
    let array = fixture.key(NodeKind::ArrayLiteralExpression, 0);
    assert_unavailable(expected_type(&fixture.db, array));
    assert_unavailable(empty_array_literal_element_abi_type(&fixture.db, array));

    let declaration = fixture.key(NodeKind::FunctionDefinition, 0);
    let instance = generic_specialization_instance(
        &fixture.db,
        declaration,
        Arc::from([GenericSubstitution::inferred("T", SemanticTypeId::U8)]),
    )
    .expect("specialization query")
    .expect("specialization instance");
    let specialized = expected_type_in_environment(&fixture.db, array, &instance)
        .expect("specialized expected type query")
        .expect("specialized expected type");
    assert!(matches!(specialized.context, ExpectedTypeContext::Return { .. }));
    assert_eq!(specialized.array_element_abi_type(), Some(SemanticTypeId::U8));
}

#[test]
fn nodes_that_are_not_bare_integers_have_no_contextual_integer_fact() {
    let fixture = Fixture::new("u8 Main(u8 b) { u8 copy = b; return copy; }");
    let path = fixture.key(NodeKind::PathExpression, 0);
    assert_eq!(fixture.integer_abi(path), None, "a local path has no integer-literal fact, available or not");
    assert_eq!(fixture.expected(path).abi_type(), SemanticTypeId::U8);
}

#[test]
fn a_block_bodied_lambda_return_expects_the_declared_function_result() {
    let fixture = Fixture::new("u8 Pick(u8 v, (u8) => u8 f) { return f(v); } u8 Main() { return Pick(9, (u8 x) => { return 2; }); }");
    let lambda = fixture.key(NodeKind::LambdaExpression, 0);
    // The lambda itself is checked against the callee parameter `(u8) => u8`.
    let own = fixture.expected(lambda);
    assert!(matches!(own.context, ExpectedTypeContext::CallArgument { position: 1, .. }), "{own:?}");
    let signature = own.function_signature().expect("a function-typed expectation");
    assert_eq!(signature.parameters.as_ref(), &[SemanticTypeId::U8]);
    assert_eq!(signature.result, SemanticTypeId::U8);
    // `return 2` leaves the lambda and takes that function type's result, never the enclosing `Main`.
    let returned = fixture.expected(fixture.literal(1));
    assert!(matches!(returned.context, ExpectedTypeContext::Return { item } if item == lambda), "{returned:?}");
    assert_eq!(returned.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(1)), Some(SemanticTypeId::U8));
}

#[test]
fn an_expression_bodied_lambda_body_expects_the_declared_function_result() {
    let fixture = Fixture::new("u8 Main() { (i64) => u8 pick = (i64 x) => 1; return pick(0); }");
    let lambda = fixture.key(NodeKind::LambdaExpression, 0);
    let body = fixture.expected(fixture.literal(0));
    assert!(matches!(body.context, ExpectedTypeContext::LambdaBody { lambda: owner } if owner == lambda), "{body:?}");
    assert_eq!(body.abi_type(), SemanticTypeId::U8);
    assert_eq!(fixture.integer_abi(fixture.literal(0)), Some(SemanticTypeId::U8));
    // The call through the local checks its argument against the function type's parameter.
    let argument = fixture.expected(fixture.literal(1));
    assert!(matches!(argument.context, ExpectedTypeContext::CallArgument { position: 0, .. }), "{argument:?}");
    assert_eq!(argument.abi_type(), SemanticTypeId::I64);
}

#[test]
fn an_untyped_lambda_parameter_takes_the_expected_parameter_type_and_nothing_without_one() {
    let fixture =
        Fixture::new("i64 Apply(i64 v, (i64) => i64 f) { return f(v); } i64 Main() { return Apply(5, x => x + 1); }");
    // Path expressions in order: `f`, `v` (Apply body), `Apply`, `x` (Main body).
    let parameter_read = fixture.key(NodeKind::PathExpression, 3);
    assert_eq!(abi_type(&fixture.db, parameter_read).expect("parameter ABI query"), Some(SemanticTypeId::I64));
    let lambda = fixture.key(NodeKind::LambdaExpression, 0);
    let signature = beskid_queries::closure_signature(&fixture.db, lambda).expect("closure query").expect("signature");
    assert_eq!(signature.callable.parameters.as_ref(), &[SemanticTypeId::I64]);
    assert_eq!(signature.callable.result, SemanticTypeId::I64);

    let orphan = Fixture::new("i64 Main() { let f = x => 1; return 0; }");
    assert_unavailable(expected_type(&orphan.db, orphan.key(NodeKind::LambdaExpression, 0)));
    assert_unavailable(beskid_queries::closure_signature(&orphan.db, orphan.key(NodeKind::LambdaExpression, 0)));
}

