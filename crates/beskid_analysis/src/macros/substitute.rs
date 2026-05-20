use std::collections::HashMap;

use crate::syntax::expressions::{BlockExpression, Expression, MacroMetavariable};
use crate::syntax::items::MacroFragmentKind;
use crate::syntax::statements::{Block, Statement};
use crate::syntax::{SpanInfo, Spanned};

/// Captured macro argument for substitution.
#[derive(Debug, Clone)]
pub enum FragmentBinding {
    Block(Spanned<Block>),
    Expression(Spanned<Expression>),
}

pub fn build_bindings(
    parameters: &[crate::syntax::items::MacroParameter],
    args: &[Spanned<Expression>],
    block: Option<&Spanned<Block>>,
) -> Result<HashMap<String, FragmentBinding>, ()> {
    let mut map = HashMap::new();
    let mut arg_index = 0usize;
    for param in parameters {
        let name = param.node.name.node.name.clone();
        let binding = match param.node.kind.node {
            MacroFragmentKind::Block => FragmentBinding::Block(block.ok_or(())?.clone()),
            MacroFragmentKind::Expression => {
                let expr = args.get(arg_index).ok_or(())?.clone();
                arg_index += 1;
                FragmentBinding::Expression(expr)
            }
            _ => return Err(()),
        };
        map.insert(name, binding);
    }
    Ok(map)
}

pub fn substitute_expression(
    expr: &Spanned<Expression>,
    bindings: &HashMap<String, FragmentBinding>,
) -> Spanned<Expression> {
    Spanned::new(substitute_expression_node(&expr.node, bindings), expr.span)
}

fn substitute_expression_node(
    expr: &Expression,
    bindings: &HashMap<String, FragmentBinding>,
) -> Expression {
    match expr {
        Expression::MacroMetavariable(mv) => {
            let key = &mv.node.name.node.name;
            if let Some(FragmentBinding::Expression(e)) = bindings.get(key) {
                return e.node.clone();
            }
            if let Some(FragmentBinding::Block(b)) = bindings.get(key) {
                return Expression::Block(Spanned::new(
                    BlockExpression { block: b.clone() },
                    b.span,
                ));
            }
            Expression::MacroMetavariable(mv.clone())
        }
        Expression::MacroInvocation(_) => expr.clone(),
        Expression::Block(b) => Expression::Block(Spanned::new(
            BlockExpression {
                block: substitute_block_spanned(&b.node.block, bindings),
            },
            b.span,
        )),
        Expression::Assign(a) => {
            let mut n = a.clone();
            n.node.target = substitute_expression(&a.node.target, bindings);
            n.node.value = substitute_expression(&a.node.value, bindings);
            Expression::Assign(n)
        }
        Expression::Binary(b) => {
            let mut n = b.clone();
            n.node.left = substitute_expression(&b.node.left, bindings);
            n.node.right = substitute_expression(&b.node.right, bindings);
            Expression::Binary(n)
        }
        Expression::Unary(u) => {
            let mut n = u.clone();
            n.node.operand = substitute_expression(&u.node.operand, bindings);
            Expression::Unary(n)
        }
        Expression::Call(c) => {
            let mut n = c.clone();
            n.node.callee = substitute_expression(&c.node.callee, bindings);
            n.node.arguments = c
                .node
                .arguments
                .iter()
                .map(|a| substitute_expression(a, bindings))
                .collect();
            Expression::Call(n)
        }
        Expression::Member(m) => {
            let mut n = m.clone();
            n.node.target = substitute_expression(&m.node.target, bindings);
            Expression::Member(n)
        }
        Expression::Grouped(g) => {
            let mut n = g.clone();
            n.node.inner = substitute_expression(&g.node.inner, bindings);
            Expression::Grouped(n)
        }
        Expression::Try(t) => {
            let mut n = t.clone();
            n.node.expr = Box::new(substitute_expression(&t.node.expr, bindings));
            Expression::Try(n)
        }
        Expression::Spawn(s) => {
            let mut n = s.clone();
            n.node.callee = substitute_expression(&s.node.callee, bindings);
            Expression::Spawn(n)
        }
        other => other.clone(),
    }
}

fn substitute_block_spanned(
    block: &Spanned<Block>,
    bindings: &HashMap<String, FragmentBinding>,
) -> Spanned<Block> {
    Spanned::new(
        Block {
            statements: block
                .node
                .statements
                .iter()
                .map(|s| substitute_statement(s, bindings))
                .collect(),
        },
        block.span,
    )
}

fn substitute_statement(
    stmt: &Spanned<Statement>,
    bindings: &HashMap<String, FragmentBinding>,
) -> Spanned<Statement> {
    let node = match &stmt.node {
        Statement::Expression(es) => Statement::Expression(Spanned::new(
            crate::syntax::ExpressionStatement {
                expression: substitute_expression(&es.node.expression, bindings),
            },
            es.span,
        )),
        Statement::Let(ls) => {
            let mut n = ls.clone();
            n.node.value = substitute_expression(&ls.node.value, bindings);
            Statement::Let(n)
        }
        Statement::Return(rs) => {
            let mut n = rs.clone();
            if let Some(v) = &rs.node.value {
                n.node.value = Some(substitute_expression(v, bindings));
            }
            Statement::Return(n)
        }
        other => other.clone(),
    };
    Spanned::new(node, stmt.span)
}

/// Expand a macro body block into a single expression suitable for expression-position invocation.
pub fn block_body_as_expression(
    body: &Spanned<Block>,
    bindings: &HashMap<String, FragmentBinding>,
    fallback_span: SpanInfo,
) -> Spanned<Expression> {
    let block = substitute_block_spanned(body, bindings);
    if block.node.statements.len() == 1 {
        if let Statement::Expression(es) = &block.node.statements[0].node {
            return es.node.expression.clone();
        }
    }
    Spanned::new(
        Expression::Block(Spanned::new(BlockExpression { block }, fallback_span)),
        fallback_span,
    )
}
