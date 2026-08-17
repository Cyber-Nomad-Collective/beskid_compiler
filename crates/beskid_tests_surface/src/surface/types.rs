//! Type surface: pest acceptance plus AST shape for happy paths; reject cases pest-only.

use beskid_analysis::Rule;
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::syntax::{EnumPath, PrimitiveType, Type};

use crate::surface::ast::{
    assert_path_segments, assert_type_complex_path, assert_type_primitive, parse_path_ast, parse_type_ast,
};
use crate::surface::util::{assert_parse, assert_parse_fail, parse_pair};

#[test]
fn primitive_type_parses_and_builds_ast() {
    assert_parse(Rule::PrimitiveType, "i32");
    let ty = parse_type_ast("i32");
    assert_type_primitive(&ty, PrimitiveType::I32);
}

#[test]
fn rejects_invalid_primitive_type() {
    assert_parse_fail(Rule::PrimitiveType, "i128");
}

#[test]
fn function_type_parses_and_builds_ast() {
    assert_parse(Rule::FunctionType, "i64(i64, i64)");
    let ty = parse_type_ast("i64(i64, i64)");
    match &ty.node {
        Type::Function { return_type, parameters } => {
            assert_type_primitive(return_type, PrimitiveType::I64);
            assert_eq!(parameters.len(), 2);
        }
        _ => panic!("expected function type"),
    }
}

#[test]
fn arrow_function_type_parses() {
    assert_parse(Rule::ArrowFunctionType, "(i64, i64) => i64");
    assert_parse(Rule::BeskidType, "(i64, i64) => i64");
}

#[test]
fn array_type_parses_and_builds_ast() {
    assert_parse(Rule::BeskidType, "i32[]");
    let ty = parse_type_ast("i32[]");
    match &ty.node {
        Type::Array(inner) => assert_type_primitive(inner, PrimitiveType::I32),
        _ => panic!("expected array type"),
    }
}

#[test]
fn path_type_parses_and_builds_ast() {
    let ty = parse_type_ast("User");
    assert_type_complex_path(&ty, &["User"]);
}

#[test]
fn array_of_complex_type_parses_and_builds_ast() {
    let ty = parse_type_ast("User[]");
    match &ty.node {
        Type::Array(inner) => assert_type_complex_path(inner, &["User"]),
        _ => panic!("expected array type"),
    }
}

#[test]
fn enum_path_parses_and_builds_ast() {
    assert_parse(Rule::EnumPath, "Option::Some");
    let pair = parse_pair(Rule::EnumPath, "Option::Some");
    let enum_path = EnumPath::parse(pair).expect("expected enum path");
    assert_eq!(enum_path.node.type_path.node.segments[0].node.name.node.name, "Option");
    assert_eq!(enum_path.node.variant.node.name, "Some");
}

#[test]
fn rejects_enum_type_path_without_variant() {
    assert_parse_fail(Rule::EnumPath, "Option:");
}

#[test]
fn path_parses_and_builds_ast() {
    let path = parse_path_ast("net.http.Client");
    assert_path_segments(&path, &["net", "http", "Client"]);
}

#[test]
fn parses_type_definition() {
    assert_parse(Rule::TypeDefinition, "type User { string name, i32 age }");
}

#[test]
fn parses_type_definition_with_conformances() {
    assert_parse(Rule::TypeDefinition, "type User : Display, Clone { string name }");
}

#[test]
fn rejects_type_definition_with_legacy_when_conformances() {
    assert_parse_fail(Rule::TypeDefinition, "type User when Display, Clone { string name }");
}

#[test]
fn parses_field_list() {
    assert_parse(Rule::FieldList, "string name, i32 age");
}

#[test]
fn rejects_field_list_without_colon() {
    assert_parse_fail(Rule::FieldList, "name: string");
}

#[test]
fn parses_type_name_with_generics() {
    assert_parse(Rule::TypeName, "Option<string>");
}

#[test]
fn parses_generic_arguments() {
    assert_parse(Rule::GenericArguments, "<i32, string>");
}

#[test]
fn rejects_generic_arguments_with_trailing_comma() {
    assert_parse_fail(Rule::GenericArguments, "<i32,>");
}

#[test]
fn rejects_empty_generic_arguments() {
    assert_parse_fail(Rule::GenericArguments, "<>");
}

#[test]
fn parses_ref_as_ordinary_type_name() {
    assert_parse(Rule::BeskidType, "ref");
}

#[test]
fn parses_event_field() {
    assert_parse(Rule::Field, "event Created(string message)");
}

#[test]
fn parses_event_field_with_capacity() {
    assert_parse(Rule::Field, "event{8} Created(string message)");
}

#[test]
fn rejects_legacy_ref_parameter_modifier() {
    assert_parse_fail(Rule::ContractDefinition, "contract C { i64 f(ref i64 p); }");
}

#[test]
fn rejects_field_without_type() {
    assert_parse_fail(Rule::FieldList, "name");
}

#[test]
fn parses_field() {
    assert_parse(Rule::Field, "string name");
}

#[test]
fn rejects_field_without_colon() {
    assert_parse_fail(Rule::Field, "name: string");
}
