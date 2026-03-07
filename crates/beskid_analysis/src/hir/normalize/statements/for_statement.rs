use crate::hir::{HirForStatement, HirStatementNode, StatementNode};
use crate::syntax::Spanned;

use crate::hir::normalize::core::Normalizer;
use crate::hir::normalize::normalizable::Normalize;

impl Normalize for Spanned<HirForStatement> {
    type Output = Vec<Spanned<HirStatementNode>>;

    fn normalize(mut self, normalizer: &mut Normalizer) -> Self::Output {
        let span = self.span;
        normalizer.visit_expression(&mut self.node.iterable);
        normalizer.visit_block(&mut self.node.body);

        vec![Spanned::new(StatementNode::ForStatement(self), span)]
    }
}
