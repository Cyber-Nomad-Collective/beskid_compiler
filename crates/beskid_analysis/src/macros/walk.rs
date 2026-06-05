//! Shared AST walks for macro expansion and substitution.

use crate::syntax::expressions::Expression;
use crate::syntax::statements::{Block, ExpressionStatement, Statement};
use crate::syntax::Spanned;

/// Map an expression bottom-up: children are transformed first, then `f` is applied to the result.
pub fn map_expression(
    expr: Spanned<Expression>,
    f: &mut impl FnMut(Spanned<Expression>) -> Spanned<Expression>,
) -> Spanned<Expression> {
    let span = expr.span;
    let mapped = match expr.node {
        Expression::Match(m) => {
            let mut n = m.clone();
            n.node.scrutinee = Box::new(map_expression(*m.node.scrutinee, f));
            for arm in &mut n.node.arms {
                if let Some(guard) = &mut arm.node.guard {
                    *guard = map_expression(guard.clone(), f);
                }
                arm.node.value = map_expression(arm.node.value.clone(), f);
            }
            Expression::Match(n)
        }
        Expression::Lambda(l) => {
            let mut n = l.clone();
            n.node.body = Box::new(map_expression(*l.node.body, f));
            Expression::Lambda(n)
        }
        Expression::Assign(a) => {
            let mut n = a.clone();
            n.node.target = Box::new(map_expression(*a.node.target, f));
            n.node.value = Box::new(map_expression(*a.node.value, f));
            Expression::Assign(n)
        }
        Expression::Binary(b) => {
            let mut n = b.clone();
            n.node.left = Box::new(map_expression(*b.node.left, f));
            n.node.right = Box::new(map_expression(*b.node.right, f));
            Expression::Binary(n)
        }
        Expression::Unary(u) => {
            let mut n = u.clone();
            n.node.expr = Box::new(map_expression(*u.node.expr, f));
            Expression::Unary(n)
        }
        Expression::Call(c) => {
            let mut n = c.clone();
            n.node.callee = Box::new(map_expression(*c.node.callee, f));
            n.node.args = c
                .node
                .args
                .iter()
                .map(|a| map_expression(a.clone(), f))
                .collect();
            Expression::Call(n)
        }
        Expression::Member(m) => {
            let mut n = m.clone();
            n.node.target = Box::new(map_expression(*m.node.target, f));
            Expression::Member(n)
        }
        Expression::Literal(_) | Expression::Path(_) => expr.node,
        Expression::MacroInvocation(_) | Expression::MacroMetavariable(_) => expr.node,
        Expression::StructLiteral(s) => {
            let mut n = s.clone();
            n.node.fields = s
                .node
                .fields
                .iter()
                .map(|field| {
                    let mut mapped_field = field.clone();
                    mapped_field.node.value = map_expression(field.node.value.clone(), f);
                    mapped_field
                })
                .collect();
            Expression::StructLiteral(n)
        }
        Expression::EnumConstructor(e) => {
            let mut n = e.clone();
            n.node.args = e
                .node
                .args
                .iter()
                .map(|a| map_expression(a.clone(), f))
                .collect();
            Expression::EnumConstructor(n)
        }
        Expression::Block(b) => {
            let mut n = b.clone();
            n.node.block = map_block(b.node.block.clone(), f);
            Expression::Block(n)
        }
        Expression::Grouped(g) => {
            let mut n = g.clone();
            n.node.expr = Box::new(map_expression(*g.node.expr, f));
            Expression::Grouped(n)
        }
        Expression::Try(t) => {
            let mut n = t.clone();
            n.node.expr = Box::new(map_expression(*t.node.expr, f));
            Expression::Try(n)
        }
        Expression::Spawn(s) => {
            let mut n = s.clone();
            n.node.callee = Box::new(map_expression(*s.node.callee, f));
            Expression::Spawn(n)
        }
        Expression::Index(i) => {
            let mut n = i.clone();
            n.node.target = Box::new(map_expression(*i.node.target, f));
            n.node.index = Box::new(map_expression(*i.node.index, f));
            Expression::Index(n)
        }
        Expression::ArrayLiteral(a) => {
            let mut n = a.clone();
            n.node.elements = a
                .node
                .elements
                .iter()
                .map(|e| map_expression(e.clone(), f))
                .collect();
            Expression::ArrayLiteral(n)
        }
    };
    f(Spanned::new(mapped, span))
}

pub fn map_block(
    block: Spanned<Block>,
    f: &mut impl FnMut(Spanned<Expression>) -> Spanned<Expression>,
) -> Spanned<Block> {
    Spanned::new(
        Block {
            statements: block
                .node
                .statements
                .iter()
                .map(|s| map_statement(s.clone(), f))
                .collect(),
        },
        block.span,
    )
}

pub fn map_statement(
    stmt: Spanned<Statement>,
    f: &mut impl FnMut(Spanned<Expression>) -> Spanned<Expression>,
) -> Spanned<Statement> {
    let span = stmt.span;
    let mapped = match stmt.node {
        Statement::Expression(es) => Statement::Expression(Spanned::new(
            ExpressionStatement {
                expression: map_expression(es.node.expression.clone(), f),
            },
            es.span,
        )),
        Statement::Let(ls) => {
            let mut n = ls.clone();
            n.node.value = map_expression(ls.node.value.clone(), f);
            Statement::Let(n)
        }
        Statement::Return(rs) => {
            let mut n = rs.clone();
            if let Some(v) = &rs.node.value {
                n.node.value = Some(map_expression(v.clone(), f));
            }
            Statement::Return(n)
        }
        Statement::If(i) => {
            let mut n = i.clone();
            n.node.condition = map_expression(i.node.condition.clone(), f);
            n.node.then_block = map_block(i.node.then_block.clone(), f);
            n.node.else_block = i
                .node
                .else_block
                .as_ref()
                .map(|b| map_block(b.clone(), f));
            Statement::If(n)
        }
        Statement::While(w) => {
            let mut n = w.clone();
            n.node.condition = map_expression(w.node.condition.clone(), f);
            n.node.body = map_block(w.node.body.clone(), f);
            Statement::While(n)
        }
        Statement::For(fr) => {
            let mut n = fr.clone();
            n.node.iterable = map_expression(fr.node.iterable.clone(), f);
            n.node.body = map_block(fr.node.body.clone(), f);
            Statement::For(n)
        }
        Statement::With(w) => {
            let mut n = w.clone();
            n.node.arguments = w
                .node
                .arguments
                .iter()
                .map(|a| map_expression(a.clone(), f))
                .collect();
            n.node.body = map_block(w.node.body.clone(), f);
            Statement::With(n)
        }
        Statement::Launch(l) => {
            let mut n = l.clone();
            n.node.arguments = l
                .node
                .arguments
                .iter()
                .map(|a| map_expression(a.clone(), f))
                .collect();
            Statement::Launch(n)
        }
        Statement::Break(_) | Statement::Continue(_) => stmt.node,
    };
    Spanned::new(mapped, span)
}
