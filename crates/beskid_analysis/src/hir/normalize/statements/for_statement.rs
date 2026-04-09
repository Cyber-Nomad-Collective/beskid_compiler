use crate::hir::{
    ExpressionNode, HirAssignExpression, HirAssignOp, HirBinaryExpression, HirBinaryOp, HirBlock,
    HirBlockExpression, HirBreakStatement, HirCallExpression, HirEnumPath, HirEnumPattern,
    HirExpressionStatement, HirForStatement, HirGroupedExpression, HirIdentifier, HirLetStatement,
    HirLiteral, HirLiteralExpression, HirMatchArm, HirMatchExpression, HirMemberExpression,
    HirPath, HirPathExpression, HirPathSegment, HirPattern, HirStatementNode, HirWhileStatement,
    StatementNode,
};
use crate::syntax::Spanned;

use crate::hir::normalize::core::Normalizer;
use crate::hir::normalize::normalizable::Normalize;

impl Normalize for Spanned<HirForStatement> {
    type Output = Vec<Spanned<HirStatementNode>>;

    fn normalize(mut self, normalizer: &mut Normalizer) -> Self::Output {
        normalizer.visit_expression(&mut self.node.iterable);
        normalizer.visit_block(&mut self.node.body);

        if extract_range_iterable(&self.node.iterable) {
            return normalize_range_fast_path(self);
        }

        normalize_generic_iterable(self)
    }
}

fn normalize_range_fast_path(for_stmt: Spanned<HirForStatement>) -> Vec<Spanned<HirStatementNode>> {
    let span = for_stmt.span;
    let (iterator, iterable, body) = (
        for_stmt.node.iterator,
        for_stmt.node.iterable,
        for_stmt.node.body,
    );
    let (start, end, range_span) = into_range_bounds(iterable);
    let iterator_name = iterator.clone();
    let end_span = shifted_span(span, 1);
    let end_binding_name = Spanned::new(
        HirIdentifier {
            name: format!("__for_end_{}", span.start),
        },
        end_span,
    );

    let init_iterator = Spanned::new(
        StatementNode::LetStatement(Spanned::new(
            HirLetStatement {
                mutable: true,
                name: iterator_name.clone(),
                type_annotation: None,
                value: start,
            },
            span,
        )),
        span,
    );

    let init_end = Spanned::new(
        StatementNode::LetStatement(Spanned::new(
            HirLetStatement {
                mutable: false,
                name: end_binding_name.clone(),
                type_annotation: None,
                value: end,
            },
            span,
        )),
        span,
    );

    let condition = Spanned::new(
        ExpressionNode::BinaryExpression(Spanned::new(
            HirBinaryExpression {
                left: Box::new(path_expr(&iterator_name)),
                op: Spanned::new(HirBinaryOp::Lt, range_span),
                right: Box::new(path_expr(&end_binding_name)),
            },
            range_span,
        )),
        range_span,
    );

    let increment = Spanned::new(
        StatementNode::ExpressionStatement(Spanned::new(
            HirExpressionStatement {
                expression: Spanned::new(
                    ExpressionNode::AssignExpression(Spanned::new(
                        HirAssignExpression {
                            target: Box::new(path_expr(&iterator_name)),
                            op: Spanned::new(HirAssignOp::Assign, iterator_name.span),
                            value: Box::new(Spanned::new(
                                ExpressionNode::BinaryExpression(Spanned::new(
                                    HirBinaryExpression {
                                        left: Box::new(path_expr(&iterator_name)),
                                        op: Spanned::new(HirBinaryOp::Add, iterator_name.span),
                                        right: Box::new(int_literal("1", iterator_name.span)),
                                    },
                                    iterator_name.span,
                                )),
                                iterator_name.span,
                            )),
                        },
                        iterator_name.span,
                    )),
                    iterator_name.span,
                ),
            },
            span,
        )),
        span,
    );

    let mut while_body = body;
    while_body.node.statements.push(increment);

    let while_stmt = Spanned::new(
        StatementNode::WhileStatement(Spanned::new(
            HirWhileStatement {
                condition,
                body: while_body,
            },
            span,
        )),
        span,
    );

    vec![init_iterator, init_end, while_stmt]
}

fn normalize_generic_iterable(
    for_stmt: Spanned<HirForStatement>,
) -> Vec<Spanned<HirStatementNode>> {
    let span = for_stmt.span;
    let (iterator, iterable, body) = (
        for_stmt.node.iterator,
        for_stmt.node.iterable,
        for_stmt.node.body,
    );
    let iter_span = shifted_span(span, 1);
    let next_span = shifted_span(span, 2);
    let next_member_span = shifted_span(span, 3);
    let loop_condition_span = shifted_span(span, 4);
    let some_arm_span = shifted_span(span, 5);
    let none_arm_span = shifted_span(span, 6);
    let match_span = shifted_span(span, 7);
    let iter_name = Spanned::new(
        HirIdentifier {
            name: format!("__for_iter_{}", span.start),
        },
        iter_span,
    );
    let next_name = Spanned::new(
        HirIdentifier {
            name: format!("__for_next_{}", span.start),
        },
        next_span,
    );

    let init_iter = Spanned::new(
        StatementNode::LetStatement(Spanned::new(
            HirLetStatement {
                mutable: true,
                name: iter_name.clone(),
                type_annotation: None,
                value: iterable,
            },
            span,
        )),
        span,
    );

    let next_call = Spanned::new(
        ExpressionNode::CallExpression(Spanned::new(
            HirCallExpression {
                callee: Box::new(Spanned::new(
                    ExpressionNode::MemberExpression(Spanned::new(
                        HirMemberExpression {
                            target: Box::new(path_expr(&iter_name)),
                            member: Spanned::new(
                                HirIdentifier {
                                    name: "Next".to_string(),
                                },
                                next_member_span,
                            ),
                        },
                        next_member_span,
                    )),
                    next_member_span,
                )),
                args: Vec::new(),
            },
            span,
        )),
        span,
    );

    let next_let = Spanned::new(
        StatementNode::LetStatement(Spanned::new(
            HirLetStatement {
                mutable: false,
                name: next_name.clone(),
                type_annotation: None,
                value: next_call,
            },
            span,
        )),
        span,
    );

    let some_arm = Spanned::new(
        HirMatchArm {
            pattern: Spanned::new(
                HirPattern::Enum(Spanned::new(
                    HirEnumPattern {
                        path: Spanned::new(
                            HirEnumPath {
                                type_name: Spanned::new(
                                    HirIdentifier {
                                        name: "Option".to_string(),
                                    },
                                    some_arm_span,
                                ),
                                variant: Spanned::new(
                                    HirIdentifier {
                                        name: "Some".to_string(),
                                    },
                                    some_arm_span,
                                ),
                            },
                            some_arm_span,
                        ),
                        items: vec![Spanned::new(
                            HirPattern::Identifier(iterator.clone()),
                            iterator.span,
                        )],
                    },
                    some_arm_span,
                )),
                some_arm_span,
            ),
            guard: None,
            value: block_expr(body, some_arm_span),
        },
        some_arm_span,
    );

    let none_arm = Spanned::new(
        HirMatchArm {
            pattern: Spanned::new(
                HirPattern::Enum(Spanned::new(
                    HirEnumPattern {
                        path: Spanned::new(
                            HirEnumPath {
                                type_name: Spanned::new(
                                    HirIdentifier {
                                        name: "Option".to_string(),
                                    },
                                    none_arm_span,
                                ),
                                variant: Spanned::new(
                                    HirIdentifier {
                                        name: "None".to_string(),
                                    },
                                    none_arm_span,
                                ),
                            },
                            none_arm_span,
                        ),
                        items: Vec::new(),
                    },
                    none_arm_span,
                )),
                none_arm_span,
            ),
            guard: None,
            value: block_expr(
                Spanned::new(
                    HirBlock {
                        statements: vec![Spanned::new(
                            StatementNode::BreakStatement(Spanned::new(HirBreakStatement, span)),
                            span,
                        )],
                    },
                    none_arm_span,
                ),
                none_arm_span,
            ),
        },
        none_arm_span,
    );

    let match_stmt = Spanned::new(
        StatementNode::ExpressionStatement(Spanned::new(
            HirExpressionStatement {
                expression: Spanned::new(
                    ExpressionNode::MatchExpression(Spanned::new(
                        HirMatchExpression {
                            scrutinee: Box::new(path_expr(&next_name)),
                            arms: vec![some_arm, none_arm],
                        },
                        match_span,
                    )),
                    match_span,
                ),
            },
            match_span,
        )),
        match_span,
    );

    let while_stmt = Spanned::new(
        StatementNode::WhileStatement(Spanned::new(
            HirWhileStatement {
                condition: bool_literal(true, loop_condition_span),
                body: Spanned::new(
                    HirBlock {
                        statements: vec![next_let, match_stmt],
                    },
                    span,
                ),
            },
            span,
        )),
        span,
    );

    vec![init_iter, while_stmt]
}

fn extract_range_iterable(iterable: &Spanned<ExpressionNode<crate::hir::HirPhase>>) -> bool {
    let ExpressionNode::CallExpression(call) = &iterable.node else {
        return false;
    };
    let ExpressionNode::PathExpression(path_expr) = &call.node.callee.node else {
        return false;
    };
    path_expr.node.path.node.segments.len() == 1
        && path_expr.node.path.node.segments[0].node.name.node.name == "range"
        && call.node.args.len() == 2
}

fn into_range_bounds(
    iterable: Spanned<ExpressionNode<crate::hir::HirPhase>>,
) -> (
    Spanned<ExpressionNode<crate::hir::HirPhase>>,
    Spanned<ExpressionNode<crate::hir::HirPhase>>,
    crate::syntax::SpanInfo,
) {
    match iterable.node {
        ExpressionNode::CallExpression(call) => {
            let mut args = call.node.args.into_iter();
            let start = args
                .next()
                .expect("range fast path requires two arguments (start)");
            let end = args
                .next()
                .expect("range fast path requires two arguments (end)");
            (start, end, call.span)
        }
        _ => panic!("range fast path expected call expression"),
    }
}

fn path_expr(name: &Spanned<HirIdentifier>) -> Spanned<ExpressionNode<crate::hir::HirPhase>> {
    Spanned::new(
        ExpressionNode::PathExpression(Spanned::new(
            HirPathExpression {
                path: Spanned::new(
                    HirPath {
                        segments: vec![Spanned::new(
                            HirPathSegment {
                                name: name.clone(),
                                type_args: Vec::new(),
                            },
                            name.span,
                        )],
                    },
                    name.span,
                ),
            },
            name.span,
        )),
        name.span,
    )
}

fn int_literal(
    value: &str,
    span: crate::syntax::SpanInfo,
) -> Spanned<ExpressionNode<crate::hir::HirPhase>> {
    Spanned::new(
        ExpressionNode::LiteralExpression(Spanned::new(
            HirLiteralExpression {
                literal: Spanned::new(HirLiteral::Integer(value.to_string()), span),
            },
            span,
        )),
        span,
    )
}

fn shifted_span(mut span: crate::syntax::SpanInfo, delta: usize) -> crate::syntax::SpanInfo {
    span.start = span.start.saturating_add(delta);
    span.end = span.end.saturating_add(delta);
    span.line_col_start = (
        span.line_col_start.0,
        span.line_col_start.1.saturating_add(delta),
    );
    span.line_col_end = (
        span.line_col_end.0,
        span.line_col_end.1.saturating_add(delta),
    );
    span
}

fn bool_literal(
    value: bool,
    span: crate::syntax::SpanInfo,
) -> Spanned<ExpressionNode<crate::hir::HirPhase>> {
    Spanned::new(
        ExpressionNode::LiteralExpression(Spanned::new(
            HirLiteralExpression {
                literal: Spanned::new(HirLiteral::Bool(value), span),
            },
            span,
        )),
        span,
    )
}

fn block_expr(
    block: Spanned<HirBlock>,
    span: crate::syntax::SpanInfo,
) -> Spanned<ExpressionNode<crate::hir::HirPhase>> {
    Spanned::new(
        ExpressionNode::GroupedExpression(Spanned::new(
            HirGroupedExpression {
                expr: Box::new(Spanned::new(
                    ExpressionNode::BlockExpression(Spanned::new(
                        HirBlockExpression { block },
                        span,
                    )),
                    span,
                )),
            },
            span,
        )),
        span,
    )
}
