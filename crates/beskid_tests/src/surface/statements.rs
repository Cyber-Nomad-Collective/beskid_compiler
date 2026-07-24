//! Statement surface: pest acceptance plus AST shape for happy paths; reject cases pest-only.

use beskid_analysis::Rule;
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::syntax::{Block, Statement};

use crate::surface::ast::{assert_expression_integer, assert_expression_path_segments, parse_statement_ast};
use crate::surface::util::{assert_parse, assert_parse_fail, parse_pair};

#[test]
fn let_statement_parses_and_builds_ast() {
    assert_parse(Rule::LetStatement, "mut i32 age = 42;");
    let statement = parse_statement_ast(Rule::LetStatement, "mut i32 age = 42;");
    match &statement.node {
        Statement::Let(let_stmt) => {
            assert!(let_stmt.node.mutable);
            assert_eq!(let_stmt.node.name.node.name, "age");
            assert!(let_stmt.node.type_annotation.is_some());
            assert_expression_integer(&let_stmt.node.value, "42");
        }
        _ => panic!("expected let statement"),
    }
}

#[test]
fn rejects_let_without_semicolon() {
    assert_parse_fail(Rule::LetStatement, "let age = 42");
}

#[test]
fn rejects_let_without_equals() {
    assert_parse_fail(Rule::LetStatement, "let age 42;");
}

#[test]
fn rejects_legacy_suffix_mut_typed_let() {
    assert_parse_fail(Rule::LetStatement, "i32 mut age = 42;");
}

#[test]
fn rejects_let_with_type_annotation() {
    assert_parse_fail(Rule::LetStatement, "let age: i32 = 42;");
}

#[test]
fn return_statement_parses_and_builds_ast() {
    assert_parse(Rule::ReturnStatement, "return 1;");
    let statement = parse_statement_ast(Rule::ReturnStatement, "return 1;");
    match &statement.node {
        Statement::Return(ret) => {
            let value = ret.node.value.as_ref().expect("return value");
            assert_expression_integer(value, "1");
        }
        _ => panic!("expected return statement"),
    }
}

#[test]
fn rejects_return_without_semicolon() {
    assert_parse_fail(Rule::ReturnStatement, "return 1");
}

#[test]
fn break_and_continue_parses_and_build_ast() {
    assert_parse(Rule::BreakStatement, "break;");
    assert_parse(Rule::ContinueStatement, "continue;");
    assert!(matches!(parse_statement_ast(Rule::BreakStatement, "break;").node, Statement::Break(_)));
    assert!(matches!(parse_statement_ast(Rule::ContinueStatement, "continue;").node, Statement::Continue(_)));
}

#[test]
fn rejects_break_without_semicolon() {
    assert_parse_fail(Rule::BreakStatement, "break");
}

#[test]
fn rejects_continue_without_semicolon() {
    assert_parse_fail(Rule::ContinueStatement, "continue");
}

#[test]
fn parses_if_statement_ast() {
    let statement = parse_statement_ast(Rule::IfStatement, "if cond { return 1; } else { return 2; }");
    match &statement.node {
        Statement::If(if_stmt) => {
            assert_expression_path_segments(&if_stmt.node.condition, &["cond"]);
            assert_eq!(if_stmt.node.then_block.node.statements.len(), 1);
            assert!(if_stmt.node.else_branch.is_some());
        }
        _ => panic!("expected if statement"),
    }
}

#[test]
fn parses_while_statement_ast() {
    let statement = parse_statement_ast(Rule::WhileStatement, "while cond { break; }");
    match &statement.node {
        Statement::While(while_stmt) => {
            assert_expression_path_segments(&while_stmt.node.condition, &["cond"]);
            assert_eq!(while_stmt.node.body.node.statements.len(), 1);
        }
        _ => panic!("expected while statement"),
    }
}

#[test]
fn parses_for_statement_ast() {
    let statement = parse_statement_ast(Rule::ForStatement, "for i in items { continue; }");
    match &statement.node {
        Statement::For(for_stmt) => {
            assert_eq!(for_stmt.node.iterator.node.name, "i");
            assert_expression_path_segments(&for_stmt.node.iterable, &["items"]);
            assert_eq!(for_stmt.node.body.node.statements.len(), 1);
        }
        _ => panic!("expected for statement"),
    }
}

#[test]
fn expression_statement_parses_and_builds_ast() {
    assert_parse(Rule::ExpressionStatement, "foo();");
    let statement = parse_statement_ast(Rule::ExpressionStatement, "foo();");
    match &statement.node {
        Statement::Expression(expr_stmt) => match &expr_stmt.node.expression.node {
            beskid_analysis::syntax::Expression::Call(call) => {
                assert!(matches!(call.node.callee.node, beskid_analysis::syntax::Expression::Path(_)));
            }
            _ => panic!("expected call expression"),
        },
        _ => panic!("expected expression statement"),
    }
}

#[test]
fn rejects_expression_statement_without_semicolon() {
    assert_parse_fail(Rule::ExpressionStatement, "foo()");
}

#[test]
fn block_parses_and_builds_ast() {
    assert_parse(Rule::Block, "{ return 1; break; }");
    let pair = parse_pair(Rule::Block, "{ return 1; break; }");
    let block = Block::parse(pair).expect("expected block");
    assert_eq!(block.node.statements.len(), 2);
}

#[test]
fn rejects_block_without_closing_brace() {
    assert_parse_fail(Rule::Block, "{ return 1;");
}

#[test]
fn parses_range_expression() {
    assert_parse(Rule::RangeExpression, "range(0, 10)");
}

#[test]
fn rejects_range_expression_without_comma() {
    assert_parse_fail(Rule::RangeExpression, "range(0 10)");
}

#[test]
fn parses_type_annotation() {
    assert_parse(Rule::TypeAnnotation, ": i32");
}

#[test]
fn rejects_type_annotation_without_type() {
    assert_parse_fail(Rule::TypeAnnotation, ":");
}

#[test]
fn rejects_statement_rule_invalid_start() {
    assert_parse_fail(Rule::Statement, "return");
}
