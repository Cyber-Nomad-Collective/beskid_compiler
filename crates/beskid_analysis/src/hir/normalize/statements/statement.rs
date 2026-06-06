use crate::hir::HirStatementNode;
use crate::syntax::Spanned;

use crate::hir::normalize::core::Normalizer;
use crate::hir::normalize::normalizable::Normalize;

impl Normalize for Spanned<HirStatementNode> {
    type Output = Vec<Spanned<HirStatementNode>>;

    fn normalize(self, normalizer: &mut Normalizer) -> Self::Output {
        let span = self.span;
        match self.node {
            HirStatementNode::LetStatement(mut let_stmt) => {
                normalizer.visit_expression(&mut let_stmt.node.value);
                vec![Spanned::new(HirStatementNode::LetStatement(let_stmt), span)]
            }
            HirStatementNode::ReturnStatement(mut return_stmt) => {
                if let Some(value) = &mut return_stmt.node.value {
                    normalizer.visit_expression(value);
                }
                vec![Spanned::new(
                    HirStatementNode::ReturnStatement(return_stmt),
                    span,
                )]
            }
            HirStatementNode::WhileStatement(mut while_stmt) => {
                normalizer.visit_expression(&mut while_stmt.node.condition);
                normalizer.visit_block(&mut while_stmt.node.body);
                vec![Spanned::new(
                    HirStatementNode::WhileStatement(while_stmt),
                    span,
                )]
            }
            HirStatementNode::ForStatement(for_stmt) => for_stmt.normalize(normalizer),
            HirStatementNode::IfStatement(mut if_stmt) => {
                normalizer.visit_if_statement(&mut if_stmt);
                vec![Spanned::new(HirStatementNode::IfStatement(if_stmt), span)]
            }
            HirStatementNode::ExpressionStatement(mut expr_stmt) => {
                normalizer.visit_expression(&mut expr_stmt.node.expression);
                vec![Spanned::new(
                    HirStatementNode::ExpressionStatement(expr_stmt),
                    span,
                )]
            }
            HirStatementNode::BreakStatement(break_stmt) => {
                vec![Spanned::new(
                    HirStatementNode::BreakStatement(break_stmt),
                    span,
                )]
            }
            HirStatementNode::ContinueStatement(continue_stmt) => {
                vec![Spanned::new(
                    HirStatementNode::ContinueStatement(continue_stmt),
                    span,
                )]
            }
            HirStatementNode::WithStatement(with_stmt) => {
                vec![Spanned::new(HirStatementNode::WithStatement(with_stmt), span)]
            }
            HirStatementNode::LaunchStatement(launch_stmt) => {
                vec![Spanned::new(HirStatementNode::LaunchStatement(launch_stmt), span)]
            }
        }
    }
}
