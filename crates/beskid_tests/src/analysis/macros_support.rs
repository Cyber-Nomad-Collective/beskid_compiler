//! Shared helpers for language macro expansion tests.

use beskid_analysis::macros::{expand_program, DEFAULT_MAX_MACRO_EXPANSION_DEPTH};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::expressions::{Expression, Literal};
use beskid_analysis::syntax::items::Node;
use beskid_analysis::syntax::statements::{Block, Statement};
use beskid_analysis::syntax::{Program, Spanned};

/// Parse `source` as `Main.bd` and run `expand_program` with the default depth cap.
pub fn parse_expand(source: &str) -> Spanned<Program> {
    parse_expand_with_depth(source, DEFAULT_MAX_MACRO_EXPANSION_DEPTH)
}

/// Parse and expand with a custom `max_depth`.
pub fn parse_expand_with_depth(source: &str, max_depth: u32) -> Spanned<Program> {
    let program = parse_program_with_source_name("Main.bd", source).expect("parse");
    expand_program(program, max_depth)
}

/// First function body named `name` in the program (top-level or inline module items).
pub fn find_function_body<'a>(program: &'a Program, name: &str) -> &'a Spanned<Block> {
    find_function_body_in_items(&program.items, name)
        .unwrap_or_else(|| panic!("function `{name}` not found"))
}

fn find_function_body_in_items<'a>(
    items: &'a [Spanned<Node>],
    name: &str,
) -> Option<&'a Spanned<Block>> {
    for item in items {
        match &item.node {
            Node::Function(f) if f.node.name.node.name == name => return Some(&f.node.body),
            Node::InlineModule(m) => {
                if let Some(body) = find_function_body_in_items(&m.node.items, name) {
                    return Some(body);
                }
            }
            _ => {}
        }
    }
    None
}

/// Walk `expr` and panic if any `MacroInvocation` remains.
pub fn assert_no_macro_invocations_in_expr(expr: &Expression) {
    walk_expression(expr, &mut |e| {
        assert!(
            !matches!(e, Expression::MacroInvocation(_)),
            "unexpected macro invocation in expression tree: {e:?}"
        );
    });
}

/// Walk all expressions under `block` and assert no macro invocations remain.
pub fn assert_no_macro_invocations_in_block(block: &Block) {
    for stmt in &block.statements {
        walk_statement(&stmt.node, &mut |e| {
            assert!(
                !matches!(e, Expression::MacroInvocation(_)),
                "unexpected macro invocation: {e:?}"
            );
        });
    }
}

/// True when `expr` contains a binary expression with integer literal `value` on either side.
pub fn expression_contains_binary_with_literal(expr: &Expression, value: i64) -> bool {
    let mut found = false;
    walk_expression(expr, &mut |e| {
        if let Expression::Binary(b) = e
            && (expr_is_integer_literal(b.node.left.as_ref(), value)
                || expr_is_integer_literal(b.node.right.as_ref(), value))
        {
            found = true;
        }
    });
    found
}

/// True when any expression under `block` contains a macro invocation named `name`.
pub fn block_contains_macro_invocation_named(block: &Block, name: &str) -> bool {
    let mut found = false;
    for stmt in &block.statements {
        walk_statement(&stmt.node, &mut |e| {
            if let Expression::MacroInvocation(inv) = e
                && inv.node.name.node.name == name
            {
                found = true;
            }
        });
    }
    found
}

/// Count macro invocations anywhere in the program item tree.
pub fn count_macro_invocations(program: &Program) -> usize {
    let mut count = 0usize;
    count_items(&program.items, &mut count);
    count
}

fn count_items(items: &[Spanned<Node>], count: &mut usize) {
    for item in items {
        match &item.node {
            Node::Function(f) => count_block(&f.node.body.node, count),
            Node::InlineModule(m) => count_items(&m.node.items, count),
            _ => {}
        }
    }
}

fn count_block(block: &Block, count: &mut usize) {
    for stmt in &block.statements {
        walk_statement(&stmt.node, &mut |e| {
            if matches!(e, Expression::MacroInvocation(_)) {
                *count += 1;
            }
        });
    }
}

fn expr_is_integer_literal(expr: &Spanned<Expression>, value: i64) -> bool {
    matches!(
        &expr.node,
        Expression::Literal(lit) if matches!(
            &lit.node.literal.node,
            Literal::Integer(text) if text.parse::<i64>().ok() == Some(value)
        )
    )
}

fn walk_statement(stmt: &Statement, visit: &mut dyn FnMut(&Expression)) {
    match stmt {
        Statement::Expression(es) => walk_expression(&es.node.expression.node, visit),
        Statement::Let(ls) => walk_expression(&ls.node.value.node, visit),
        Statement::Return(rs) => {
            if let Some(v) = &rs.node.value {
                walk_expression(&v.node, visit);
            }
        }
        Statement::If(i) => {
            walk_expression(&i.node.condition.node, visit);
            walk_block(&i.node.then_block.node, visit);
            if let Some(else_b) = &i.node.else_branch {
                match &else_b.node {
                    beskid_analysis::syntax::ElseBranch::Block(block) => {
                        walk_block(&block.node, visit);
                    }
                    beskid_analysis::syntax::ElseBranch::If(nested) => {
                        walk_expression(&nested.node.condition.node, visit);
                        walk_block(&nested.node.then_block.node, visit);
                    }
                }
            }
        }
        Statement::While(w) => {
            walk_expression(&w.node.condition.node, visit);
            walk_block(&w.node.body.node, visit);
        }
        Statement::For(f) => {
            walk_expression(&f.node.iterable.node, visit);
            walk_block(&f.node.body.node, visit);
        }
        Statement::With(w) => walk_block(&w.node.body.node, visit),
        _ => {}
    }
}

fn walk_block(block: &Block, visit: &mut dyn FnMut(&Expression)) {
    for stmt in &block.statements {
        walk_statement(&stmt.node, visit);
    }
}

fn walk_expression(expr: &Expression, visit: &mut dyn FnMut(&Expression)) {
    visit(expr);
    match expr {
        Expression::Block(b) => walk_block(&b.node.block.node, visit),
        Expression::Assign(a) => {
            walk_expression(&a.node.target.node, visit);
            walk_expression(&a.node.value.node, visit);
        }
        Expression::Binary(b) => {
            walk_expression(&b.node.left.node, visit);
            walk_expression(&b.node.right.node, visit);
        }
        Expression::Unary(u) => walk_expression(&u.node.expr.node, visit),
        Expression::Call(c) => {
            walk_expression(&c.node.callee.node, visit);
            for arg in &c.node.args {
                walk_expression(&arg.node, visit);
            }
        }
        Expression::Member(m) => walk_expression(&m.node.target.node, visit),
        Expression::Grouped(g) => walk_expression(&g.node.expr.node, visit),
        Expression::Try(t) => walk_expression(&t.node.expr.node, visit),
        Expression::Spawn(s) => walk_expression(&s.node.callee.node, visit),
        Expression::Match(m) => {
            walk_expression(&m.node.scrutinee.node, visit);
            for arm in &m.node.arms {
                walk_expression(&arm.node.value.node, visit);
            }
        }
        Expression::Lambda(l) => walk_expression(&l.node.body.node, visit),
        _ => {}
    }
}
