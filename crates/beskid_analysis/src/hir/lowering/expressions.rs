use crate::hir::{
    HirArrayLiteralExpression, HirAssignExpression, HirAssignOp, HirBinaryExpression,
    HirBlockExpression, HirCallExpression, HirEnumConstructorExpression, HirEnumPattern,
    HirExpressionNode, HirGroupedExpression, HirIndexExpression, HirLambdaExpression,
    HirLambdaParameter, HirLiteral, HirLiteralExpression, HirMatchArm, HirMatchExpression,
    HirMemberExpression, HirPathExpression, HirPattern, HirSpawnExpression,
    HirStructLiteralExpression, HirStructLiteralField, HirTryExpression, HirUnaryExpression,
};
use crate::syntax::{self, Spanned};

use super::Lowerable;

impl Lowerable for Spanned<syntax::Expression> {
    type Output = Spanned<HirExpressionNode>;

    fn lower(&self) -> Self::Output {
        let node = match &self.node {
            syntax::Expression::Match(match_expr) => {
                HirExpressionNode::MatchExpression(match_expr.lower())
            }
            syntax::Expression::Lambda(lambda_expr) => {
                HirExpressionNode::LambdaExpression(lambda_expr.lower())
            }
            syntax::Expression::Assign(assign_expr) => {
                HirExpressionNode::AssignExpression(assign_expr.lower())
            }
            syntax::Expression::Binary(binary_expr) => {
                HirExpressionNode::BinaryExpression(binary_expr.lower())
            }
            syntax::Expression::Unary(unary_expr) => {
                HirExpressionNode::UnaryExpression(unary_expr.lower())
            }
            syntax::Expression::Call(call_expr) => {
                HirExpressionNode::CallExpression(call_expr.lower())
            }
            syntax::Expression::Member(member_expr) => {
                HirExpressionNode::MemberExpression(member_expr.lower())
            }
            syntax::Expression::Literal(literal_expr) => {
                HirExpressionNode::LiteralExpression(literal_expr.lower())
            }
            syntax::Expression::Path(path_expr) => {
                HirExpressionNode::PathExpression(path_expr.lower())
            }
            syntax::Expression::StructLiteral(struct_expr) => {
                HirExpressionNode::StructLiteralExpression(struct_expr.lower())
            }
            syntax::Expression::EnumConstructor(enum_expr) => {
                HirExpressionNode::EnumConstructorExpression(enum_expr.lower())
            }
            syntax::Expression::Block(block_expr) => {
                HirExpressionNode::BlockExpression(block_expr.lower())
            }
            syntax::Expression::Grouped(grouped_expr) => {
                HirExpressionNode::GroupedExpression(grouped_expr.lower())
            }
            syntax::Expression::Try(try_expr) => HirExpressionNode::TryExpression(try_expr.lower()),
            syntax::Expression::Spawn(spawn_expr) => {
                HirExpressionNode::SpawnExpression(spawn_expr.lower())
            }
            syntax::Expression::MacroInvocation(m) => HirExpressionNode::MacroInvocation(m.clone()),
            syntax::Expression::MacroMetavariable(m) => {
                HirExpressionNode::MacroMetavariable(m.clone())
            }
            syntax::Expression::Index(node) => {
                let hir = HirIndexExpression {
                    target: Box::new(node.node.target.lower()),
                    index: Box::new(node.node.index.lower()),
                };
                HirExpressionNode::IndexExpression(Spanned::new(hir, node.span))
            }
            syntax::Expression::ArrayLiteral(node) => {
                let hir = HirArrayLiteralExpression {
                    elements: node.node.elements.iter().map(Lowerable::lower).collect(),
                };
                HirExpressionNode::ArrayLiteralExpression(Spanned::new(hir, node.span))
            }
            syntax::Expression::CodeString(code) => lower_code_string_expression(code),
        };
        Spanned::new(node, self.span)
    }
}

fn lower_code_string_expression(
    code: &Spanned<syntax::CodeStringLiteral>,
) -> HirExpressionNode {
    use super::Lowerable;
    use crate::hir::{HirBinaryExpression, HirBinaryOp, HirLiteral, HirLiteralExpression};
    use crate::services::parse_expression_source;

    let mut expr: Option<Spanned<HirExpressionNode>> = None;
    for segment in &code.node.segments {
        let part = match segment {
            syntax::CodeStringSegment::Text(text) => HirExpressionNode::LiteralExpression(
                Spanned::new(
                    HirLiteralExpression {
                        literal: Spanned::new(HirLiteral::String(text.clone()), code.span),
                    },
                    code.span,
                ),
            ),
            syntax::CodeStringSegment::Hole(source) => match parse_expression_source("@code-hole", source) {
                Ok(parsed) => parsed.lower().node,
                Err(_) => HirExpressionNode::LiteralExpression(Spanned::new(
                    HirLiteralExpression {
                        literal: Spanned::new(HirLiteral::String(String::new()), code.span),
                    },
                    code.span,
                )),
            },
        };
        expr = Some(match expr {
            None => Spanned::new(part, code.span),
            Some(left) => Spanned::new(
                HirExpressionNode::BinaryExpression(Spanned::new(
                    HirBinaryExpression {
                        left: Box::new(left),
                        op: Spanned::new(HirBinaryOp::Add, code.span),
                        right: Box::new(Spanned::new(part, code.span)),
                    },
                    code.span,
                )),
                code.span,
            ),
        });
    }
    expr.unwrap_or_else(|| {
        Spanned::new(
            HirExpressionNode::LiteralExpression(Spanned::new(
                HirLiteralExpression {
                    literal: Spanned::new(HirLiteral::String(String::new()), code.span),
                },
                code.span,
            )),
            code.span,
        )
    })
    .node
}

impl Lowerable for Spanned<syntax::SpawnExpression> {
    type Output = Spanned<HirSpawnExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirSpawnExpression {
                callee: Box::new(self.node.callee.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::IndexExpression> {
    type Output = Spanned<HirIndexExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirIndexExpression {
                target: Box::new(self.node.target.lower()),
                index: Box::new(self.node.index.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::ArrayLiteralExpression> {
    type Output = Spanned<HirArrayLiteralExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirArrayLiteralExpression {
                elements: self.node.elements.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::LambdaExpression> {
    type Output = Spanned<HirLambdaExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirLambdaExpression {
                parameters: self.node.parameters.iter().map(Lowerable::lower).collect(),
                body: Box::new(self.node.body.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::LambdaParameter> {
    type Output = Spanned<HirLambdaParameter>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirLambdaParameter {
                name: self.node.name.lower(),
                ty: self.node.ty.as_ref().map(Lowerable::lower),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::MatchArm> {
    type Output = Spanned<HirMatchArm>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirMatchArm {
                pattern: self.node.pattern.lower(),
                guard: self.node.guard.as_ref().map(Lowerable::lower),
                value: self.node.value.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::Pattern> {
    type Output = Spanned<HirPattern>;

    fn lower(&self) -> Self::Output {
        let node = match &self.node {
            syntax::Pattern::Wildcard => HirPattern::Wildcard,
            syntax::Pattern::Identifier(identifier) => HirPattern::Identifier(identifier.lower()),
            syntax::Pattern::Literal(literal) => HirPattern::Literal(literal.lower()),
            syntax::Pattern::Enum(enum_pattern) => HirPattern::Enum(enum_pattern.lower()),
        };
        Spanned::new(node, self.span)
    }
}

impl Lowerable for Spanned<syntax::MatchExpression> {
    type Output = Spanned<HirMatchExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirMatchExpression {
                scrutinee: Box::new(self.node.scrutinee.lower()),
                arms: self.node.arms.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::AssignExpression> {
    type Output = Spanned<HirAssignExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirAssignExpression {
                target: Box::new(self.node.target.lower()),
                op: Spanned::new(
                    match self.node.op.node {
                        syntax::AssignOp::Assign => HirAssignOp::Assign,
                        syntax::AssignOp::AddAssign => HirAssignOp::AddAssign,
                        syntax::AssignOp::SubAssign => HirAssignOp::SubAssign,
                    },
                    self.node.op.span,
                ),
                value: Box::new(self.node.value.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::BinaryExpression> {
    type Output = Spanned<HirBinaryExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirBinaryExpression {
                left: Box::new(self.node.left.lower()),
                op: self.node.op.lower(),
                right: Box::new(self.node.right.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::UnaryExpression> {
    type Output = Spanned<HirUnaryExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirUnaryExpression {
                op: self.node.op.lower(),
                expr: Box::new(self.node.expr.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::CallExpression> {
    type Output = Spanned<HirCallExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirCallExpression {
                callee: Box::new(self.node.callee.lower()),
                args: self.node.args.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::MemberExpression> {
    type Output = Spanned<HirMemberExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirMemberExpression {
                target: Box::new(self.node.target.lower()),
                member: self.node.member.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::LiteralExpression> {
    type Output = Spanned<HirLiteralExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirLiteralExpression {
                literal: self.node.literal.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::PathExpression> {
    type Output = Spanned<HirPathExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirPathExpression {
                path: self.node.path.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::StructLiteralExpression> {
    type Output = Spanned<HirStructLiteralExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirStructLiteralExpression {
                path: self.node.path.lower(),
                fields: self.node.fields.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::EnumConstructorExpression> {
    type Output = Spanned<HirEnumConstructorExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirEnumConstructorExpression {
                path: self.node.path.lower(),
                args: self.node.args.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::BlockExpression> {
    type Output = Spanned<HirBlockExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirBlockExpression {
                block: self.node.block.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::GroupedExpression> {
    type Output = Spanned<HirGroupedExpression>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirGroupedExpression {
                expr: Box::new(self.node.expr.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::TryExpression> {
    type Output = Spanned<HirTryExpression>;

    fn lower(&self) -> Self::Output {
        // Pipeline contract: lowering preserves explicit `TryExpression` shape so typing can
        // perform centralized validity checks before normalization desugars it to match control-flow.
        Spanned::new(
            HirTryExpression {
                expr: Box::new(self.node.expr.lower()),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::Literal> {
    type Output = Spanned<HirLiteral>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            match &self.node {
                syntax::Literal::Integer(value) => HirLiteral::Integer(value.clone()),
                syntax::Literal::Float(value) => HirLiteral::Float(value.clone()),
                syntax::Literal::String(value) => HirLiteral::String(value.clone()),
                syntax::Literal::Char(value) => HirLiteral::Char(value.clone()),
                syntax::Literal::Bool(value) => HirLiteral::Bool(*value),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::StructLiteralField> {
    type Output = Spanned<HirStructLiteralField>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirStructLiteralField {
                name: self.node.name.lower(),
                value: self.node.value.lower(),
            },
            self.span,
        )
    }
}

impl Lowerable for Spanned<syntax::EnumPattern> {
    type Output = Spanned<HirEnumPattern>;

    fn lower(&self) -> Self::Output {
        Spanned::new(
            HirEnumPattern {
                path: self.node.path.lower(),
                items: self.node.items.iter().map(Lowerable::lower).collect(),
            },
            self.span,
        )
    }
}
