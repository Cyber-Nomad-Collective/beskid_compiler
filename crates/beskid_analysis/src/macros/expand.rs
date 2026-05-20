use crate::syntax::expressions::{Expression, MacroInvocation};
use crate::syntax::items::{Node, Program};
use crate::syntax::statements::{Block, Statement};
use crate::syntax::{Spanned};

use super::registry::{MacroRegistry, macro_name_key};
use super::substitute::{block_body_as_expression, build_bindings, substitute_block_spanned};

pub const DEFAULT_MAX_MACRO_EXPANSION_DEPTH: u32 = 32;

/// Expand all `name!` invocations in `program` up to `max_depth` rounds.
pub fn expand_program(program: Spanned<Program>, max_depth: u32) -> Spanned<Program> {
    let mut current = program;
    for _ in 0..max_depth {
        let registry = MacroRegistry::from_program(&current.node);
        let (next, changed) = expand_once(current, &registry);
        current = next;
        if !changed {
            break;
        }
    }
    current
}

fn expand_once(
    program: Spanned<Program>,
    registry: &MacroRegistry,
) -> (Spanned<Program>, bool) {
    let mut changed = false;
    let items = program
        .node
        .items
        .iter()
        .map(|item| expand_node(item, registry, &mut changed))
        .collect();
    (
        Spanned::new(
            Program {
                items,
                leading_docs: program.node.leading_docs.clone(),
            },
            program.span,
        ),
        changed,
    )
}

fn expand_node(
    item: &Spanned<Node>,
    registry: &MacroRegistry,
    changed: &mut bool,
) -> Spanned<Node> {
    match &item.node {
        Node::Function(f) => {
            let mut n = f.clone();
            n.node.body = expand_block(&f.node.body, registry, changed);
            Spanned::new(Node::Function(n), item.span)
        }
        Node::InlineModule(m) => {
            let mut n = m.clone();
            n.node.items = n
                .node
                .items
                .iter()
                .map(|i| expand_node(i, registry, changed))
                .collect();
            Spanned::new(Node::InlineModule(n), item.span)
        }
        _ => item.clone(),
    }
}

fn expand_block(
    block: &Spanned<Block>,
    registry: &MacroRegistry,
    changed: &mut bool,
) -> Spanned<Block> {
    Spanned::new(
        Block {
            statements: block
                .node
                .statements
                .iter()
                .map(|s| expand_statement(s, registry, changed))
                .collect(),
        },
        block.span,
    )
}

fn expand_statement(
    stmt: &Spanned<Statement>,
    registry: &MacroRegistry,
    changed: &mut bool,
) -> Spanned<Statement> {
    match &stmt.node {
        Statement::Expression(es) => {
            let expr = expand_expression(&es.node.expression, registry, changed);
            Statement::Expression(Spanned::new(
                crate::syntax::ExpressionStatement { expression: expr },
                es.span,
            ))
        }
        Statement::Let(ls) => {
            let mut n = ls.clone();
            if let Some(init) = &ls.node.initializer {
                n.node.initializer = Some(expand_expression(init, registry, changed));
            }
            Statement::Let(n)
        }
        Statement::Return(rs) => {
            let mut n = rs.clone();
            if let Some(v) = &rs.node.value {
                n.node.value = Some(expand_expression(v, registry, changed));
            }
            Statement::Return(n)
        }
        other => other.clone(),
    }
}

fn expand_statement_spanned(
    stmt: &Spanned<Statement>,
    registry: &MacroRegistry,
    changed: &mut bool,
) -> Spanned<Statement> {
    Spanned::new(expand_statement(stmt, registry, changed), stmt.span)
}

fn expand_expression(
    expr: &Spanned<Expression>,
    registry: &MacroRegistry,
    changed: &mut bool,
) -> Spanned<Expression> {
    match &expr.node {
        Expression::MacroInvocation(inv) => {
            if let Some(expanded) = expand_invocation(inv, registry) {
                *changed = true;
                expand_expression(&expanded, registry, changed)
            } else {
                expr.clone()
            }
        }
        Expression::Block(b) => {
            let mut n = b.clone();
            n.node.block = expand_block(&b.node.block, registry, changed);
            Spanned::new(Expression::Block(n), expr.span)
        }
        Expression::Assign(a) => {
            let mut n = a.clone();
            n.node.target = expand_expression(&a.node.target, registry, changed);
            n.node.value = expand_expression(&a.node.value, registry, changed);
            Spanned::new(Expression::Assign(n), expr.span)
        }
        Expression::Binary(b) => {
            let mut n = b.clone();
            n.node.left = expand_expression(&b.node.left, registry, changed);
            n.node.right = expand_expression(&b.node.right, registry, changed);
            Spanned::new(Expression::Binary(n), expr.span)
        }
        Expression::Unary(u) => {
            let mut n = u.clone();
            n.node.operand = expand_expression(&u.node.operand, registry, changed);
            Spanned::new(Expression::Unary(n), expr.span)
        }
        Expression::Call(c) => {
            let mut n = c.clone();
            n.node.callee = expand_expression(&c.node.callee, registry, changed);
            n.node.arguments = c
                .node
                .arguments
                .iter()
                .map(|a| expand_expression(a, registry, changed))
                .collect();
            Spanned::new(Expression::Call(n), expr.span)
        }
        Expression::Member(m) => {
            let mut n = m.clone();
            n.node.target = expand_expression(&m.node.target, registry, changed);
            Spanned::new(Expression::Member(n), expr.span)
        }
        Expression::Grouped(g) => {
            let mut n = g.clone();
            n.node.inner = expand_expression(&g.node.inner, registry, changed);
            Spanned::new(Expression::Grouped(n), expr.span)
        }
        Expression::Try(t) => {
            let mut n = t.clone();
            n.node.expr = Box::new(expand_expression(&t.node.expr, registry, changed));
            Spanned::new(Expression::Try(n), expr.span)
        }
        Expression::Spawn(s) => {
            let mut n = s.clone();
            n.node.callee = expand_expression(&s.node.callee, registry, changed);
            Spanned::new(Expression::Spawn(n), expr.span)
        }
        other => expr.clone(),
    }
}

fn expand_invocation(
    inv: &Spanned<MacroInvocation>,
    registry: &MacroRegistry,
) -> Option<Spanned<Expression>> {
    let name = macro_name_key(&inv.node.name);
    let def = registry.get(&name)?;
    let bindings = build_bindings(
        &def.node.parameters,
        &inv.node.arguments,
        inv.node.block.as_ref(),
    )
    .ok()?;
    Some(block_body_as_expression(
        &def.node.body,
        &bindings,
        inv.span,
    ))
}
