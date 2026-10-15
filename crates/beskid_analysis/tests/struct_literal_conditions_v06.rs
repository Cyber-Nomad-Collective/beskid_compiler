//! A struct literal is not allowed at the top level of an `if`/`while` condition, a `for` iterable
//! or a `match` scrutinee, because the `{` that follows those positions opens the body:
//! `if a == b {}` is the path `b` and an empty block. Nested bracketed positions (groups, calls,
//! subscripts, array literals, blocks, match arms, interpolations) allow struct literals again, and
//! a parenthesized struct literal is allowed in every restricted position.

use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{Expression, Node, Spanned, Statement};

fn parse_statements(body: &str) -> (String, Vec<Spanned<Statement>>) {
    let source = format!("type P {{ i64 x }} unit Run(i64 a, i64 b, P p, i64[] xs) {{ {body} }}");
    let program = parse_program(&source).unwrap_or_else(|error| panic!("failed to parse {source:?}: {error:?}"));
    let Node::Function(function) = &program.node.items.last().expect("function").node else {
        panic!("expected a function in {source:?}");
    };
    let statements = function.node.body.node.statements.clone();
    (source, statements)
}

fn text<'a, T>(source: &'a str, node: &Spanned<T>) -> &'a str {
    source[node.span.start..node.span.end].trim_end()
}

fn single_if(body: &str) -> (String, Spanned<Expression>, usize) {
    let (source, statements) = parse_statements(body);
    let Statement::If(statement) = &statements.first().expect("statement").node else {
        panic!("expected an if statement in {source:?}");
    };
    let condition = statement.node.condition.clone();
    let then_len = statement.node.then_block.node.statements.len();
    (source, condition, then_len)
}

fn binary_right(condition: &Spanned<Expression>) -> &Spanned<Expression> {
    let Expression::Binary(binary) = &condition.node else {
        panic!("condition: {:?}", condition.node);
    };
    &binary.node.right
}

#[test]
fn empty_block_after_equality_with_name_is_a_block() {
    let (source, condition, then_len) = single_if("if a == b {} return;");
    assert_eq!(text(&source, &condition), "a == b");
    assert!(matches!(binary_right(&condition).node, Expression::Path(_)), "right: {:?}", condition.node);
    assert_eq!(then_len, 0);
}

#[test]
fn non_empty_block_after_equality_with_name_is_a_block() {
    let (source, condition, then_len) = single_if("if a == b { return; }");
    assert_eq!(text(&source, &condition), "a == b");
    assert!(matches!(binary_right(&condition).node, Expression::Path(_)));
    assert_eq!(then_len, 1);
}

#[test]
fn empty_block_after_qualified_path_is_a_block() {
    let (source, condition, then_len) = single_if("if a == Core.Limits.Max {}");
    assert_eq!(text(&source, &condition), "a == Core.Limits.Max");
    assert_eq!(then_len, 0);
}

#[test]
fn empty_block_after_call_and_member_is_a_block() {
    let (source, condition, _) = single_if("if Check(a) {}");
    assert!(matches!(condition.node, Expression::Call(_)), "condition: {:?}", condition.node);
    assert_eq!(text(&source, &condition), "Check(a)");

    let (source, condition, _) = single_if("if p.x == b {}");
    assert_eq!(text(&source, &condition), "p.x == b");
}

#[test]
fn bare_name_condition_with_empty_block_is_a_block() {
    let (source, condition, then_len) = single_if("if b {} else {}");
    assert!(matches!(condition.node, Expression::Path(_)), "condition: {:?}", condition.node);
    assert_eq!(text(&source, &condition), "b");
    assert_eq!(then_len, 0);
}

#[test]
fn parenthesized_struct_literal_is_allowed_in_a_condition() {
    let (_, condition, then_len) = single_if("if (P { x: 1 }) == p {}");
    let Expression::Binary(binary) = &condition.node else {
        panic!("condition: {:?}", condition.node);
    };
    let Expression::Grouped(grouped) = &binary.node.left.node else {
        panic!("left: {:?}", binary.node.left.node);
    };
    assert!(matches!(grouped.node.expr.node, Expression::StructLiteral(_)), "grouped: {:?}", grouped.node);
    assert_eq!(then_len, 0);
}

#[test]
fn struct_literal_in_call_arguments_is_allowed_in_a_condition() {
    let (_, condition, _) = single_if("if Same(P { x: 1 }, p) {}");
    let Expression::Call(call) = &condition.node else {
        panic!("condition: {:?}", condition.node);
    };
    assert!(matches!(call.node.args[0].node, Expression::StructLiteral(_)), "argument: {:?}", call.node.args[0].node);
}

#[test]
fn struct_literal_in_array_literal_and_subscript_is_allowed_in_a_condition() {
    let (_, condition, _) = single_if("if Count([P { x: 1 }]) == xs[Index(P { x: 2 })] {}");
    assert!(matches!(condition.node, Expression::Binary(_)), "condition: {:?}", condition.node);
}

#[test]
fn struct_literal_in_a_block_body_is_allowed() {
    let (_, statements) = parse_statements("if a == b { P q = P { x: 1 }; }");
    let Statement::If(statement) = &statements[0].node else { panic!("expected if") };
    assert_eq!(statement.node.then_block.node.statements.len(), 1);
}

#[test]
fn struct_literal_outside_conditions_is_unchanged() {
    let (_, statements) = parse_statements("P q = P { x: 1 }; P r = P {};");
    assert_eq!(statements.len(), 2);
    for statement in &statements {
        let Statement::Let(statement) = &statement.node else { panic!("expected let") };
        let value = &statement.node.value;
        assert!(matches!(value.node, Expression::StructLiteral(_)), "value: {:?}", value.node);
    }
}

#[test]
fn while_condition_with_empty_block() {
    let (source, statements) = parse_statements("while a < b {} return;");
    assert_eq!(statements.len(), 2);
    let Statement::While(statement) = &statements[0].node else { panic!("expected while") };
    assert_eq!(text(&source, &statement.node.condition), "a < b");
    assert!(statement.node.body.node.statements.is_empty());
}

#[test]
fn for_iterable_with_empty_block() {
    let (source, statements) = parse_statements("for x in xs {} return;");
    assert_eq!(statements.len(), 2);
    let Statement::For(statement) = &statements[0].node else { panic!("expected for") };
    assert!(matches!(statement.node.iterable.node, Expression::Path(_)));
    assert_eq!(text(&source, &statement.node.iterable), "xs");
    assert!(statement.node.body.node.statements.is_empty());
}

#[test]
fn match_scrutinee_followed_by_arms() {
    let (source, statements) = parse_statements("match b {}; match b { _ => P { x: 1 }, };");
    assert_eq!(statements.len(), 2);
    for (index, statement) in statements.iter().enumerate() {
        let Statement::Expression(statement) = &statement.node else { panic!("expected expression statement") };
        let Expression::Match(matched) = &statement.node.expression.node else {
            panic!("expected match: {:?}", statement.node.expression.node);
        };
        assert!(matches!(matched.node.scrutinee.node, Expression::Path(_)));
        assert_eq!(text(&source, &matched.node.scrutinee), "b");
        assert_eq!(matched.node.arms.len(), index);
        if let Some(arm) = matched.node.arms.first() {
            assert!(matches!(arm.node.value.node, Expression::StructLiteral(_)), "arm: {:?}", arm.node.value.node);
        }
    }
}

#[test]
fn parenthesized_struct_literal_is_allowed_as_scrutinee_and_iterable() {
    let (_, statements) = parse_statements("match (P { x: 1 }) { _ => 0, }; for y in (Items(P { x: 1 })) {}");
    assert_eq!(statements.len(), 2);
}

#[test]
fn unparenthesized_struct_literal_condition_is_rejected() {
    let source = "type P { i64 x } unit Run(P p) { if p == P { x: 1 } { return; } }";
    assert!(parse_program(source).is_err(), "a top-level struct literal in a condition must be parenthesized");
}

#[test]
fn nested_condition_inside_a_lambda_block_in_a_call_keeps_its_own_context() {
    let (_, statements) = parse_statements("if Any(xs, x => { if x == b {} return P { x: 1 } == p; }) {}");
    let Statement::If(statement) = &statements[0].node else { panic!("expected if") };
    assert!(matches!(statement.node.condition.node, Expression::Call(_)));
}
