//! Keywords and primitive type names match only at a word boundary, so identifiers that start
//! with a keyword (`clif_xor`, `spawnIt`, `returnValue`, `u8count`) stay identifiers.

use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{Node, Statement};

fn statements(source: &str) -> Vec<Statement> {
    let program = parse_program(source).unwrap_or_else(|error| panic!("failed to parse {source:?}: {error:?}"));
    let Node::Function(function) = &program.node.items.last().expect("function").node else {
        panic!("expected a function in {source:?}");
    };
    function.node.body.node.statements.iter().map(|statement| statement.node.clone()).collect()
}

fn single_statement(body: &str) -> Statement {
    let mut found = statements(&format!("unit Run() {{ {body} }}"));
    assert_eq!(found.len(), 1, "expected one statement in {body:?}");
    found.remove(0)
}

#[test]
fn keyword_prefixed_identifiers_are_identifiers() {
    for name in [
        "clif_xor", "in_range", "host_name", "hostName", "spawnIt", "neverName", "returnValue", "breakCount",
        "continueAt", "matchCount", "trueValue", "falseHood", "useCount", "modulus", "typeName", "testCase",
        "iffy", "format", "whileLoop", "pubKey", "launchPad", "scopeId", "initialized", "withdraw", "letter",
        "mutable", "bulky", "This_", "u8count", "i32x", "bool_ok", "stringify", "unity", "neverland", "pointerish",
        "wordy", "charset", "f64bits",
    ] {
        let program = parse_program(&format!("i64 {name}() {{ return 0; }}"))
            .unwrap_or_else(|error| panic!("`{name}` must parse as a function name: {error:?}"));
        let Node::Function(function) = &program.node.items[0].node else { panic!("function {name}") };
        assert_eq!(function.node.name.node.name, name);
    }
}

#[test]
fn keyword_prefixed_assignments_are_not_declarations() {
    for body in ["u8count = 1;", "bool_ok = true;", "returnValue = 1;", "neverName = 2;", "stringCount = 3;"] {
        assert!(
            matches!(single_statement(body), Statement::Expression(_)),
            "`{body}` must be an assignment expression, not a typed declaration"
        );
    }
}

#[test]
fn keyword_prefixed_calls_are_calls() {
    for body in ["spawnIt(1);", "clif_xor(1);", "returnValue(1);", "matchAll(1);", "trueValue(1);"] {
        assert!(matches!(single_statement(body), Statement::Expression(_)), "`{body}` must be a call statement");
    }
}

#[test]
fn keywords_still_parse_at_word_boundaries() {
    assert!(matches!(single_statement("return;"), Statement::Return(_)));
    assert!(matches!(single_statement("return 1;"), Statement::Return(_)));
    assert!(matches!(single_statement("u8 count = 1;"), Statement::Let(_)));
    assert!(matches!(single_statement("bool ok = true;"), Statement::Let(_)));
    assert!(matches!(single_statement("u8[] bytes = [];"), Statement::Let(_)));
    assert!(matches!(single_statement("let x = spawn Work();"), Statement::Let(_)));
    assert!(matches!(single_statement("while true { break; }"), Statement::While(_)));
    assert!(matches!(single_statement("for item in items { continue; }"), Statement::For(_)));
    assert!(parse_program("pub type Point { i64 x, i64 y, }").is_ok());
    assert!(parse_program("i64 F(i64 v) { return clif { return %0 }; }").is_ok());
    assert!(parse_program("i64 F(i64 v) { return match v { 0 => 1, _ => 2, }; }").is_ok());
    assert!(parse_program("use Core.Results as R;").is_ok());
}

#[test]
fn bare_keywords_remain_reserved() {
    use beskid_analysis::parser::{BeskidParser, Rule};
    use pest::Parser;
    for source in ["i64 return() { return 0; }", "i64 clif() { return 0; }", "unit Run() { i64 match = 1; }"] {
        assert!(BeskidParser::parse(Rule::Program, source).is_err(), "unexpectedly accepted {source}");
    }
}
