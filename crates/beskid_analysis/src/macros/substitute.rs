use std::collections::HashMap;

use crate::syntax::expressions::{Expression, MacroMetavariable};
use crate::syntax::items::MacroParameter;
use crate::syntax::statements::{Block, Statement};
use crate::syntax::{Identifier, Spanned};

/// Replace `$param` metavariables in `source` using captured fragment clones.
pub fn substitute_block(
    source: &Block,
    bindings: &HashMap<String, FragmentBinding>,
) -> Block {
    Block {
        statements: source
            .statements
            .iter()
            .map(|stmt| Spanned::new(substitute_statement(&stmt.node, bindings), stmt.span))
            .collect(),
    }
}

pub fn substitute_expression(
    source: &Expression,
    bindings: &HashMap<String, FragmentBinding>,
) -> Expression {
    match source {
        Expression::MacroMetavariable(mv) => {
            let key = mv.node.name.node.name.clone();
            if let Some(binding) = bindings.get(&key) {
                return binding.as_expression().unwrap_or_else(|| {
                    Expression::MacroMetavariable(mv.clone())
                });
            }
            Expression::MacroMetavariable(mv.clone())
        }
        Expression::Block(b) => Expression::Block(Spanned::new(
            BlockExpressionFromBlock(substitute_block(&b.node.block, bindings)),
            b.span,
        )),
        Expression::Assign(a) => Expression::Assign(Spanned::new(
            crate::syntax::AssignExpression {
                target: Spanned::new(
                    substitute_expression(&a.node.target.node, bindings),
                    a.node.target.span,
                ),
                op: a.node.op,
                value: Spanned::new(
                    substitute_expression(&a.node.value.node, bindings),
                    a.node.value.span,
                ),
            },
            a.span,
        )),
        Expression::Binary(b) => {
            let mut lowered = b.clone();
            lowered.node.left = Spanned::new(
                substitute_expression(&b.node.left.node, bindings),
                b.node.left.span,
            );
            lowered.node.right = Spanned::new(
                substitute_expression(&b.node.right.node, bindings),
                b.node.right.span,
            );
            Expression::Binary(lowered)
        }
        Expression::Unary(u) => {
            let mut lowered = u.clone();
            lowered.node.operand = Spanned::new(
                substitute_expression(&u.node.operand.node, bindings),
                u.node.operand.span,
            );
            Expression::Unary(lowered)
        }
        Expression::Call(c) => {
            let mut lowered = c.clone();
            lowered.node.callee = Spanned::new(
                substitute_expression(&c.node.callee.node, bindings),
                c.node.callee.span,
            );
            lowered.node.arguments = c
                .node
                .arguments
                .iter()
                .map(|arg| {
                    Spanned::new(substitute_expression(&arg.node, bindings), arg.span)
                })
                .collect();
            Expression::Call(lowered)
        }
        Expression::Member(m) => {
            let mut lowered = m.clone();
            lowered.node.target = Spanned::new(
                substitute_expression(&m.node.target.node, bindings),
                m.node.target.span,
            );
            Expression::Member(lowered)
        }
        Expression::Literal(l) => Expression::Literal(l.clone()),
        Expression::Path(p) => Expression::Path(p.clone()),
        Expression::StructLiteral(s) => Expression::StructLiteral(s.clone()),
        Expression::EnumConstructor(e) => Expression::EnumConstructor(e.clone()),
        Expression::Grouped(g) => {
            let mut lowered = g.clone();
            lowered.node.inner = Spanned::new(
                substitute_expression(&g.node.inner.node, bindings),
                g.node.inner.span,
            );
            Expression::Grouped(lowered)
        }
        Expression::Try(t) => {
            let mut lowered = t.clone();
            lowered.node.expr = Box::new(Spanned::new(
                substitute_expression(&t.node.expr.node, bindings),
                t.node.expr.span,
            ));
            Expression::Try(lowered)
        }
        Expression::Spawn(s) => {
            let mut lowered = s.clone();
            lowered.node.callee = Spanned::new(
                substitute_expression(&s.node.callee.node, bindings),
                s.node.callee.span,
            );
            Expression::Spawn(lowered)
        }
        Expression::Match(m) => Expression::Match(m.clone()),
        Expression::Lambda(l) => Expression::Lambda(l.clone()),
        Expression::MacroInvocation(_) => source.clone(),
    }
}

fn substitute_statement(stmt: &Statement, bindings: &HashMap<String, FragmentBinding>) -> Statement {
    match stmt {
        Statement::Expression(es) => Statement::Expression(Spanned::new(
            crate::syntax::ExpressionStatement {
                expression: Spanned::new(
                    substitute_expression(&es.node.expression.node, bindings),
                    es.node.expression.span,
                ),
            },
            es.span,
        )),
        Statement::Let(ls) => {
            let mut lowered = ls.clone();
            if let Some(ref init) = ls.node.initializer {
                lowered.node.initializer = Some(Spanned::new(
                    substitute_expression(&init.node, bindings),
                    init.span,
                ));
            }
            Statement::Let(lowered)
        }
        Statement::Return(rs) => {
            let mut lowered = rs.clone();
            if let Some(ref value) = rs.node.value {
                lowered.node.value = Some(Spanned::new(
                    substitute_expression(&value.node, bindings),
                    value.span,
                ));
            }
            Statement::Return(lowered)
        }
        Statement::If(i) => Statement::If(i.clone()),
        Statement::While(w) => Statement::While(w.clone()),
        Statement::For(f) => Statement::For(f.clone()),
        Statement::Break(b) => Statement::Break(b.clone()),
        Statement::Continue(c) => Statement::Continue(c.clone()),
    }
}

/// Helper: wrap substituted block as block expression.
fn BlockExpressionFromBlock(block: Block) -> crate::syntax::BlockExpression {
    crate::syntax::BlockExpression { block }
}

/// Captured macro argument fragment.
#[derive(Debug, Clone)]
pub enum FragmentBinding {
    Block(Block),
    Expression(Expression),
    Statement(Statement),
    Identifier(Spanned<Identifier>),
}

impl FragmentBinding {
    pub fn from_parameter(
        param: &MacroParameter,
        args: &[Spanned<Expression>],
        block: Option<&Spanned<Block>>,
        arg_index: &mut usize,
    ) -> Result<Self, ()> {
        use crate::syntax::items::MacroFragmentKind;
        let kind = param.node.kind.node;
        match kind {
            MacroFragmentKind::Block => {
                let b = block.ok_or(())?.node.clone();
                Ok(FragmentBinding::Block(b))
            }
            MacroFragmentKind::Expression => {
                let e = args.get(*arg_index).ok_or(())?.node.clone();
                *arg_index += 1;
                Ok(FragmentBinding::Expression(e))
            }
            MacroFragmentKind::Statement => {
                let e = args.get(*arg_index).ok_or(())?.node.clone();
                *arg_index += 1;
                Ok(FragmentBinding::Statement(statement_from_expression(e)?))
            }
            MacroFragmentKind::Identifier => {
                let e = args.get(*arg_index).ok_or(())?.node.clone();
                *arg_index += 1;
                Ok(FragmentBinding::Identifier(identifier_from_expression(e)?))
            }
            _ => Err(()),
        }
    }

    pub fn as_expression(&self) -> Option<Expression> {
        match self {
            FragmentBinding::Expression(e) => Some(e.clone()),
            FragmentBinding::Block(b) => Some(Expression::Block(Spanned::new(
                BlockExpressionFromBlock(b.clone()),
                crate::syntax::SpanInfo::dummy(),
            ))),
            FragmentBinding::Statement(s) => statement_as_expression(s),
            FragmentBinding::Identifier(id) => Some(Expression::Path(Spanned::new(
                crate::syntax::PathExpression {
                    path: Spanned::new(
                        crate::syntax::Path {
                            segments: vec![id.clone()],
                        },
                        id.span,
                    ),
                },
                id.span,
            ))),
        }
    }
}

fn statement_from_expression(expr: Expression) -> Result<Statement, ()> {
    Ok(Statement::Expression(Spanned::new(
        crate::syntax::ExpressionStatement {
            expression: Spanned::new(expr, crate::syntax::SpanInfo::dummy()),
        },
        crate::syntax::SpanInfo::dummy(),
    )))
}

fn statement_as_expression(stmt: &Statement) -> Option<Expression> {
    match stmt {
        Statement::Expression(es) => Some(es.node.expression.node.clone()),
        _ => None,
    }
}

fn identifier_from_expression(expr: Expression) -> Result<Spanned<Identifier>, ()> {
    match expr {
        Expression::Path(p) if p.node.path.node.segments.len() == 1 => {
            Ok(p.node.path.node.segments[0].clone())
        }
        _ => Err(()),
    }
}
