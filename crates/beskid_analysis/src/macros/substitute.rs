//! Apply macro fragment bindings to syntax trees.

use std::collections::HashMap;

use crate::syntax::expressions::{BlockExpression, Expression, LiteralExpression, PathExpression};
use crate::syntax::statements::{Block, Statement};
use crate::syntax::{SpanInfo, Spanned};

use super::match_args::FragmentBinding;
use super::walk::map_block;

pub type Bindings = HashMap<String, FragmentBinding>;

pub fn bindings_from_pairs(pairs: Vec<(String, FragmentBinding)>) -> Bindings {
    pairs.into_iter().collect()
}

pub fn substitute_block(block: &Spanned<Block>, bindings: &Bindings) -> Spanned<Block> {
    map_block(block.clone(), &mut |mapped| {
        if let Expression::MacroMetavariable(mv) = &mapped.node
            && let Some(binding) = bindings.get(&mv.node.name.node.name)
        {
            return binding_to_expression(binding, mv.span);
        }
        mapped
    })
}

fn binding_to_expression(binding: &FragmentBinding, fallback_span: SpanInfo) -> Spanned<Expression> {
    match binding {
        FragmentBinding::Expression(e) => e.clone(),
        FragmentBinding::Block(b) => {
            Spanned::new(Expression::Block(Spanned::new(BlockExpression { block: b.clone() }, b.span)), fallback_span)
        }
        FragmentBinding::Statement(s) => match &s.node {
            Statement::Expression(es) => es.node.expression.clone(),
            _ => Spanned::new(
                Expression::Block(Spanned::new(
                    BlockExpression { block: Spanned::new(Block { statements: vec![s.clone()] }, s.span) },
                    s.span,
                )),
                fallback_span,
            ),
        },
        FragmentBinding::Literal(lit) => Spanned::new(
            Expression::Literal(Spanned::new(LiteralExpression { literal: lit.clone() }, lit.span)),
            fallback_span,
        ),
        FragmentBinding::Path(path) => Spanned::new(
            Expression::Path(Spanned::new(PathExpression { path: path.clone() }, path.span)),
            fallback_span,
        ),
        FragmentBinding::Identifier(id) => Spanned::new(
            Expression::Path(Spanned::new(
                PathExpression {
                    path: Spanned::new(
                        crate::syntax::Path {
                            segments: vec![Spanned::new(
                                crate::syntax::PathSegment { name: id.clone(), type_args: Vec::new() },
                                id.span,
                            )],
                        },
                        id.span,
                    ),
                },
                id.span,
            )),
            fallback_span,
        ),
        FragmentBinding::Type(ty) => match &ty.node {
            crate::syntax::Type::Complex(path) => Spanned::new(
                Expression::Path(Spanned::new(PathExpression { path: path.clone() }, path.span)),
                fallback_span,
            ),
            _ => Spanned::new(
                Expression::Path(Spanned::new(
                    PathExpression { path: Spanned::new(crate::syntax::Path { segments: Vec::new() }, fallback_span) },
                    fallback_span,
                )),
                fallback_span,
            ),
        },
        FragmentBinding::Node(e) => e.clone(),
        FragmentBinding::Pattern(_) | FragmentBinding::Item(_) => mapped_metavariable_placeholder(fallback_span),
    }
}

fn mapped_metavariable_placeholder(span: SpanInfo) -> Spanned<Expression> {
    Spanned::new(
        Expression::MacroMetavariable(Spanned::new(
            crate::syntax::MacroMetavariable {
                name: Spanned::new(crate::syntax::Identifier { name: "_fragment".to_string() }, span),
            },
            span,
        )),
        span,
    )
}

/// Expand a macro body block into a single expression suitable for expression-position invocation.
pub fn block_body_as_expression(
    body: &Spanned<Block>,
    bindings: &Bindings,
    fallback_span: SpanInfo,
) -> Spanned<Expression> {
    let block = substitute_block(body, bindings);
    if block.node.statements.len() == 1
        && let Statement::Expression(es) = &block.node.statements[0].node
    {
        return es.node.expression.clone();
    }
    Spanned::new(Expression::Block(Spanned::new(BlockExpression { block }, fallback_span)), fallback_span)
}
