use crate::errors::CodegenError;
use crate::lowering::composition::{lower_launch_statement, lower_with_statement};
use crate::lowering::composition_policy::RUNTIME_CONTAINER_LOWERING_ENABLED;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use beskid_analysis::hir::HirStatementNode;
use beskid_analysis::syntax::Spanned;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirStatementNode {
    type Output = ();

    fn lower(node: &Spanned<Self>, ctx: &mut NodeLoweringContext<'_, '_>) -> Result<Self::Output, CodegenError> {
        match &node.node {
            HirStatementNode::LetStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::ReturnStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::BreakStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::ContinueStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::WhileStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::ForStatement(_) => {
                Err(CodegenError::UnsupportedNode { span: node.span, node: "for statement" })
            }
            HirStatementNode::IfStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::ExpressionStatement(inner) => lower_node(inner, ctx),
            HirStatementNode::WithStatement(inner) => {
                if RUNTIME_CONTAINER_LOWERING_ENABLED {
                    lower_with_statement(inner, ctx)
                } else {
                    Ok(())
                }
            }
            HirStatementNode::LaunchStatement(inner) => {
                if RUNTIME_CONTAINER_LOWERING_ENABLED {
                    lower_launch_statement(inner, ctx)
                } else {
                    Ok(())
                }
            }
        }
    }
}
