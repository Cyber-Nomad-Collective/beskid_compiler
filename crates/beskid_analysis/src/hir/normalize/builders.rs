use crate::hir::{
    HirCallExpression, HirEnumPath, HirEnumPattern, HirExpressionNode, HirIdentifier, HirLiteral,
    HirLiteralExpression, HirMatchArm, HirMatchExpression, HirPath, HirPathExpression,
    HirPathSegment, HirPattern, HirTryExpression,
};
use crate::syntax::{SpanInfo, Spanned};

const TRY_OK_BINDING_OFFSET: usize = 10;
const TRY_OK_PATTERN_OFFSET: usize = 11;
const TRY_ERR_ARM_OFFSET: usize = 12;
const TRY_MATCH_OFFSET: usize = 13;
const TRY_PANIC_CALLEE_OFFSET: usize = 20;
const TRY_PANIC_MSG_OFFSET: usize = 21;

pub(super) fn hir_path_expr(name: &str, span: SpanInfo) -> Spanned<HirExpressionNode> {
    Spanned::new(
        HirExpressionNode::PathExpression(Spanned::new(
            HirPathExpression {
                path: Spanned::new(
                    HirPath {
                        segments: vec![Spanned::new(
                            HirPathSegment {
                                name: hir_identifier(name, span),
                                type_args: Vec::new(),
                            },
                            span,
                        )],
                    },
                    span,
                ),
            },
            span,
        )),
        span,
    )
}

pub(super) fn desugar_try_expression(
    try_expr: Spanned<HirTryExpression>,
    parent_span: SpanInfo,
) -> Spanned<HirExpressionNode> {
    let ok_binding_span = offset_span(parent_span, TRY_OK_BINDING_OFFSET);
    let ok_pattern_span = offset_span(parent_span, TRY_OK_PATTERN_OFFSET);
    let wildcard_arm_span = offset_span(parent_span, TRY_ERR_ARM_OFFSET);
    let match_span = offset_span(parent_span, TRY_MATCH_OFFSET);
    let ok_binding = hir_identifier(format!("__try_ok_{}", parent_span.start), ok_binding_span);

    let ok_arm = Spanned::new(
        HirMatchArm {
            pattern: Spanned::new(
                HirPattern::Enum(Spanned::new(
                    HirEnumPattern {
                        path: Spanned::new(
                            HirEnumPath {
                                type_name: hir_identifier("Result", ok_pattern_span),
                                variant: hir_identifier("Ok", ok_pattern_span),
                            },
                            ok_pattern_span,
                        ),
                        items: vec![Spanned::new(
                            HirPattern::Identifier(ok_binding.clone()),
                            ok_binding_span,
                        )],
                    },
                    ok_pattern_span,
                )),
                ok_pattern_span,
            ),
            guard: None,
            value: hir_path_expr(&ok_binding.node.name, ok_binding_span),
        },
        ok_pattern_span,
    );

    let err_arm = Spanned::new(
        HirMatchArm {
            pattern: Spanned::new(HirPattern::Wildcard, wildcard_arm_span),
            guard: None,
            value: hir_panic_call_expr(wildcard_arm_span),
        },
        wildcard_arm_span,
    );

    Spanned::new(
        HirExpressionNode::MatchExpression(Spanned::new(
            HirMatchExpression {
                scrutinee: try_expr.node.expr,
                arms: vec![ok_arm, err_arm],
            },
            match_span,
        )),
        match_span,
    )
}

fn offset_span(base: SpanInfo, offset: usize) -> SpanInfo {
    SpanInfo {
        start: base.start.saturating_add(offset),
        end: base.end.saturating_add(offset),
        line_col_start: base.line_col_start,
        line_col_end: base.line_col_end,
    }
}

fn hir_identifier(name: impl Into<String>, span: SpanInfo) -> Spanned<HirIdentifier> {
    Spanned::new(HirIdentifier { name: name.into() }, span)
}

fn hir_panic_call_expr(span: SpanInfo) -> Spanned<HirExpressionNode> {
    Spanned::new(
        HirExpressionNode::CallExpression(Spanned::new(
            HirCallExpression {
                callee: Box::new(hir_path_expr(
                    "__panic_str",
                    offset_span(span, TRY_PANIC_CALLEE_OFFSET),
                )),
                args: vec![Spanned::new(
                    HirExpressionNode::LiteralExpression(Spanned::new(
                        HirLiteralExpression {
                            literal: Spanned::new(
                                HirLiteral::String("try expression propagated error".to_string()),
                                offset_span(span, TRY_PANIC_MSG_OFFSET),
                            ),
                        },
                        offset_span(span, TRY_PANIC_MSG_OFFSET),
                    )),
                    offset_span(span, TRY_PANIC_MSG_OFFSET),
                )],
            },
            span,
        )),
        span,
    )
}
