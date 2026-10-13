//! Match facts for the full 0.6 match surface: nested matches over generic enum payload
//! bindings, nested generic enum patterns, `when` guards, and by-value matches over primitive
//! scrutinees.

use super::support::{key, key_at_start, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AggregateFieldShape, EnumMatchPatternFact, IndexedNodeKind, ManagedReferenceKind, SemanticTypeId, enum_match,
    node_kind, node_type, scalar_match,
};

const OPTION_RESULT: &str = "enum Option<T> { Some(T value), None } \
     enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } ";

/// The ProcessSuite shape: `match input.TryWrite(...) { Result::Ok(sent) => match sent { ... } }`.
/// The inner scrutinee is a pattern binding of the generic payload `Option<i64>`, whose declaration
/// has no layout until applied; the binding's applied identity is the authority.
#[test]
fn nested_match_on_a_generic_enum_payload_binding_uses_the_applied_payload_layout() {
    for (signature, scrutinee) in [
        ("i64 Main(Result<Option<i64>, i64> result)", "result"),
        ("Result<Option<i64>, i64> TryWrite() { return Result::Error(0_i64); } i64 Main()", "TryWrite()"),
    ] {
        let source = format!(
            "{OPTION_RESULT}{signature} {{ return match {scrutinee} {{ \
             Result::Ok(sent) => match sent {{ Option::Some(bytes) => bytes, Option::None => -1_i64, }}, \
             Result::Error(_) => -2_i64, }}; }}"
        );
        let (db, _project, unit, generation, index) = setup(&source);
        let inner = key_at_start(unit, generation, &index, NodeKind::MatchExpression, source.find("match sent").unwrap());
        let outer = key_at_start(
            unit,
            generation,
            &index,
            NodeKind::MatchExpression,
            source.find(&format!("match {scrutinee}")).unwrap(),
        );

        let fact = enum_match(&db, inner)
            .unwrap_or_else(|error| panic!("{scrutinee}: inner match query: {error:?}"))
            .expect("inner match fact over the applied Option<i64> payload");
        assert_eq!(fact.layout.variants[0].fields[0].1, AggregateFieldShape::Scalar(SemanticTypeId::I64), "{scrutinee}");
        let EnumMatchPatternFact::Enum(some) = &fact.arms[0].pattern else {
            panic!("{scrutinee}: Some arm must remain an enum pattern");
        };
        let EnumMatchPatternFact::Binding(bytes) = &some.items[0] else {
            panic!("{scrutinee}: Some payload must bind");
        };
        assert_eq!(bytes.payload, AggregateFieldShape::Scalar(SemanticTypeId::I64), "{scrutinee}");
        assert_eq!(bytes.managed_reference, ManagedReferenceKind::NativeOrScalar, "{scrutinee}");
        assert_eq!(node_type(&db, inner).expect("inner type"), Some(SemanticTypeId::I64), "{scrutinee}");
        assert_eq!(node_type(&db, outer).expect("outer type"), Some(SemanticTypeId::I64), "{scrutinee}");
    }
}

/// `Result::Ok(Option::Some(bytes))`: a nested enum pattern over a generic payload resolves the
/// payload's applied layout the same way.
#[test]
fn nested_enum_pattern_over_a_generic_payload_uses_the_applied_payload_layout() {
    let source = format!(
        "{OPTION_RESULT}i64 Main(Result<Option<i64>, i64> result) {{ return match result {{ \
         Result::Ok(Option::Some(bytes)) => bytes, Result::Ok(Option::None) => -1_i64, Result::Error(_) => -2_i64, }}; }}"
    );
    let (db, _project, unit, generation, index) = setup(&source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let fact = enum_match(&db, expression).expect("match query").expect("nested generic pattern fact");
    let EnumMatchPatternFact::Enum(ok) = &fact.arms[0].pattern else {
        panic!("Ok arm");
    };
    let EnumMatchPatternFact::Enum(some) = &ok.items[0] else {
        panic!("nested Some pattern");
    };
    assert_eq!(some.layout.variants[0].fields[0].1, AggregateFieldShape::Scalar(SemanticTypeId::I64));
    let EnumMatchPatternFact::Binding(bytes) = &some.items[0] else {
        panic!("nested payload binding");
    };
    assert_eq!(bytes.payload, AggregateFieldShape::Scalar(SemanticTypeId::I64));
}

#[test]
fn guarded_arms_retain_their_normalized_guard_expression() {
    let source = "enum Choice { Value(i64 value), Empty } i64 Main(Choice choice) { return match choice { \
                  Choice::Value(v) when v > 0_i64 => v, Choice::Value(_) => 0_i64, Choice::Empty => -1_i64, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    let fact = enum_match(&db, expression).expect("match query").expect("guarded match fact");
    let guard = fact.arms[0].guard.expect("the first arm keeps its guard");
    assert_eq!(node_kind(&db, guard).expect("guard kind"), Some(IndexedNodeKind::BinaryExpression));
    assert_eq!(node_type(&db, guard).expect("guard type"), Some(SemanticTypeId::BOOL));
    assert!(fact.arms[1].guard.is_none() && fact.arms[2].guard.is_none());
    assert_eq!(node_type(&db, expression).expect("match type"), Some(SemanticTypeId::I64));
}

#[test]
fn primitive_scrutinee_matches_have_a_scalar_match_fact() {
    let source = "i64 Main(i64 count, string name) { \
                  i64 first = match count { 0 => 1_i64, 7_i64 => 2_i64, other => other, }; \
                  return match name { \"a\" => first, _ => 0_i64, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let integer = key_at_start(unit, generation, &index, NodeKind::MatchExpression, source.find("match count").unwrap());
    let text = key_at_start(unit, generation, &index, NodeKind::MatchExpression, source.find("match name").unwrap());

    let integer_fact = scalar_match(&db, integer).expect("integer match query").expect("integer match fact");
    assert_eq!(integer_fact.scrutinee_type, SemanticTypeId::I64);
    let EnumMatchPatternFact::ScalarLiteral(zero) = &integer_fact.arms[0].pattern else {
        panic!("unsuffixed literal pattern");
    };
    assert_eq!(zero.semantic_type, SemanticTypeId::I64, "an unsuffixed literal takes the scrutinee type");
    let EnumMatchPatternFact::ScalarLiteral(seven) = &integer_fact.arms[1].pattern else {
        panic!("suffixed literal pattern");
    };
    assert_eq!(seven.semantic_type, SemanticTypeId::I64);
    let EnumMatchPatternFact::Binding(other) = &integer_fact.arms[2].pattern else {
        panic!("whole-scrutinee binding");
    };
    assert_eq!(other.payload, AggregateFieldShape::Scalar(SemanticTypeId::I64));
    assert_eq!(node_type(&db, integer).expect("integer match type"), Some(SemanticTypeId::I64));

    let text_fact = scalar_match(&db, text).expect("string match query").expect("string match fact");
    assert_eq!(text_fact.scrutinee_type, SemanticTypeId::STRING);
    let EnumMatchPatternFact::ScalarLiteral(literal) = &text_fact.arms[0].pattern else {
        panic!("string literal pattern");
    };
    assert_eq!(literal.semantic_type, SemanticTypeId::STRING);
    assert!(matches!(text_fact.arms[1].pattern, EnumMatchPatternFact::Wildcard));
    assert_eq!(node_type(&db, text).expect("string match type"), Some(SemanticTypeId::I64));
}

#[test]
fn enum_scrutinee_matches_have_no_scalar_match_fact() {
    let source = "enum Choice { Value(i64 value), Empty } i64 Main(Choice choice) { return match choice { \
                  Choice::Value(v) => v, Choice::Empty => -1_i64, }; }";
    let (db, _project, unit, generation, index) = setup(source);
    let expression = key(unit, generation, &index, NodeKind::MatchExpression, 0);
    assert_eq!(scalar_match(&db, expression).expect("scalar match query"), None);
    assert!(enum_match(&db, expression).expect("enum match query").is_some());
}
