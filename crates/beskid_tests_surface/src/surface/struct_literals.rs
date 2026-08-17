use crate::surface::util::{assert_parse, assert_parse_fail};
use beskid_analysis::Rule;

#[test]
fn parses_struct_literal_expression() {
    let input = "User { name: \"Ada\", age: 37 }";
    assert_parse(Rule::StructLiteralExpression, input);
}

#[test]
fn parses_struct_literal_with_nullary_enum_constructor_field() {
    assert_parse(Rule::StructLiteralExpression, "Foo { x: Option::None }");
    assert_parse(Rule::StructLiteralExpression, "QueryState<T> { count: 0, first: Option::None }");
}

#[test]
fn rejects_struct_literal_without_fields() {
    assert_parse_fail(Rule::StructLiteralExpression, "User { name \"Ada\" }");
}

#[test]
fn rejects_struct_literal_without_comma_between_fields() {
    assert_parse_fail(Rule::StructLiteralExpression, "User { name: \"Ada\" age: 37 }");
}

#[test]
fn parses_field_value_list() {
    assert_parse(Rule::FieldValueList, "name: \"Ada\", age: 37");
}

#[test]
fn rejects_field_value_list_without_colon() {
    assert_parse_fail(Rule::FieldValueList, "name \"Ada\"");
}

#[test]
fn parses_field_value() {
    assert_parse(Rule::FieldValue, "name: 1");
}

#[test]
fn parses_expression_nullary_enum_before_struct_literal_close() {
    assert_parse(Rule::Expression, "Option::None");
    assert_parse(Rule::Expression, " Option::None ");
    assert_parse(Rule::Expression, "Option::None ");
}

#[test]
fn parses_field_value_list_with_nullary_enum_constructor() {
    assert_parse(Rule::FieldValueList, "x: Option::None");
}

#[test]
fn parses_field_value_with_nullary_enum_constructor() {
    assert_parse(Rule::FieldValue, "first: Option::None");
    assert_parse(Rule::Expression, "Option::None");
}

#[test]
fn rejects_field_value_without_colon() {
    assert_parse_fail(Rule::FieldValue, "name 1");
}

#[test]
fn parses_struct_literal_with_nullary_enum_constructor_with_parens() {
    assert_parse(Rule::StructLiteralExpression, "Foo { x: Option::None() }");
}
