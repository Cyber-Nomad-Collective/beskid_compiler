//! Match arm guards end at the arm's `=>`. A guard that starts like lambda parameters
//! (`when strict => ...`) is the guard expression, not an unparenthesized lambda that swallows the
//! arm body.

use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::{Expression, MatchArm, Node, Spanned, Statement};

fn parse_arms(source: &str) -> Vec<Spanned<MatchArm>> {
    let program = parse_program(source).unwrap_or_else(|error| panic!("failed to parse {source:?}: {error:?}"));
    let Node::Function(function) = &program.node.items.last().expect("function").node else {
        panic!("expected a function in {source:?}");
    };
    let statement = function.node.body.node.statements.first().expect("statement");
    let Statement::Expression(statement) = &statement.node else {
        panic!("expected an expression statement in {source:?}");
    };
    let Expression::Match(matched) = &statement.node.expression.node else {
        panic!("expected a match expression in {source:?}");
    };
    matched.node.arms.clone()
}

fn match_source(arms: &str) -> String {
    format!("unit Run(Option<i64> v, bool strict) {{ match v {{ {arms} }}; }}")
}

fn guard_text<'a>(source: &'a str, arm: &Spanned<MatchArm>) -> &'a str {
    let guard = arm.node.guard.as_ref().expect("guard");
    // Expression spans keep the trailing whitespace before `=>`, as for every other expression.
    source[guard.span.start..guard.span.end].trim_end()
}

#[test]
fn bare_name_guard_parses_as_guard() {
    let source = match_source("Option::Some(x) when strict => { x; }, _ => { }");
    let arms = parse_arms(&source);
    assert_eq!(arms.len(), 2);
    let guard = arms[0].node.guard.as_ref().expect("guard");
    assert!(matches!(guard.node, Expression::Path(_)), "guard: {:?}", guard.node);
    assert_eq!(guard_text(&source, &arms[0]), "strict");
    assert!(matches!(arms[0].node.value.node, Expression::Block(_)), "value: {:?}", arms[0].node.value.node);
}

#[test]
fn bare_name_guard_with_expression_body_parses() {
    let source = match_source("Option::Some(x) when strict => x, _ => 0");
    let arms = parse_arms(&source);
    assert_eq!(guard_text(&source, &arms[0]), "strict");
    assert!(matches!(arms[0].node.value.node, Expression::Path(_)), "value: {:?}", arms[0].node.value.node);
}

#[test]
fn parenthesized_name_guard_parses_as_guard() {
    let source = match_source("Option::Some(x) when (strict) => x, _ => 0");
    let arms = parse_arms(&source);
    let guard = arms[0].node.guard.as_ref().expect("guard");
    assert!(matches!(guard.node, Expression::Grouped(_)), "guard: {:?}", guard.node);
    assert_eq!(guard_text(&source, &arms[0]), "(strict)");
}

#[test]
fn comparison_guard_parses() {
    let source = match_source("Option::Some(x) when x > 0 => x, _ => 0");
    let arms = parse_arms(&source);
    let guard = arms[0].node.guard.as_ref().expect("guard");
    assert!(matches!(guard.node, Expression::Binary(_)), "guard: {:?}", guard.node);
    assert_eq!(guard_text(&source, &arms[0]), "x > 0");
}

#[test]
fn call_guard_parses() {
    let source = match_source("Option::Some(x) when IsSmall(x) => x, _ => 0");
    let arms = parse_arms(&source);
    let guard = arms[0].node.guard.as_ref().expect("guard");
    assert!(matches!(guard.node, Expression::Call(_)), "guard: {:?}", guard.node);
    assert_eq!(guard_text(&source, &arms[0]), "IsSmall(x)");
}

#[test]
fn guard_with_parenthesized_lambda_parses() {
    let source = match_source("Option::Some(x) when Check((y => y > 0), x) => x, _ => 0");
    let arms = parse_arms(&source);
    let guard = arms[0].node.guard.as_ref().expect("guard");
    let Expression::Call(call) = &guard.node else {
        panic!("guard: {:?}", guard.node);
    };
    let Expression::Grouped(grouped) = &call.node.args[0].node else {
        panic!("first argument: {:?}", call.node.args[0].node);
    };
    assert!(matches!(grouped.node.expr.node, Expression::Lambda(_)), "grouped: {:?}", grouped.node);
    assert_eq!(guard_text(&source, &arms[0]), "Check((y => y > 0), x)");
}

#[test]
fn lambda_in_arm_body_still_parses() {
    let source = match_source("Option::Some(x) when strict => (y => y + x), _ => (y => y)");
    let arms = parse_arms(&source);
    assert_eq!(guard_text(&source, &arms[0]), "strict");
    let Expression::Grouped(grouped) = &arms[0].node.value.node else {
        panic!("value: {:?}", arms[0].node.value.node);
    };
    assert!(matches!(grouped.node.expr.node, Expression::Lambda(_)), "grouped: {:?}", grouped.node);

    let source = match_source("Option::Some(x) => y => y + x, _ => y => y");
    let arms = parse_arms(&source);
    assert!(arms[0].node.guard.is_none());
    assert!(matches!(arms[0].node.value.node, Expression::Lambda(_)), "value: {:?}", arms[0].node.value.node);
    assert!(matches!(arms[1].node.value.node, Expression::Lambda(_)), "value: {:?}", arms[1].node.value.node);
}
