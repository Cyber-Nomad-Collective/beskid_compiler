use crate::errors::CodegenError;
use crate::lowering::expressions::call_expression::lower_lambda_function_value;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use beskid_analysis::hir::HirExpressionNode;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::Value;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirExpressionNode {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        match &node.node {
            HirExpressionNode::MatchExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::LambdaExpression(lambda) => {
                Ok(Some(lower_lambda_function_value(lambda, node.span, ctx)?))
            }
            HirExpressionNode::AssignExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::BinaryExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::UnaryExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::CallExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::MemberExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::LiteralExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::PathExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::StructLiteralExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::EnumConstructorExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::BlockExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::GroupedExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::SpawnExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::TryExpression(inner) => {
                // Invariant: normalization should always desugar try to match before codegen.
                debug_assert!(
                    ctx.expr_type(inner.node.expr.span).is_some(),
                    "unexpected raw TryExpression reached codegen; expected upstream desugaring"
                );
                Err(CodegenError::UnsupportedNode {
                    span: node.span,
                    node: "unexpected raw try expression",
                })
            }
            HirExpressionNode::MacroInvocation(_) => Err(CodegenError::UnsupportedNode {
                span: node.span,
                node: "macro invocation expression",
            }),
            HirExpressionNode::MacroMetavariable(_) => Err(CodegenError::UnsupportedNode {
                span: node.span,
                node: "macro metavariable expression",
            }),
            HirExpressionNode::IndexExpression(inner) => lower_node(inner, ctx),
            HirExpressionNode::ArrayLiteralExpression(inner) => lower_node(inner, ctx),
        }
    }
}
