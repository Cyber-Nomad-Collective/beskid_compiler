use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use beskid_analysis::hir::HirBlockExpression;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::Value;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirBlockExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, crate::errors::CodegenError> {
        let saved_locals = ctx.state.locals.clone();
        let prior_terminated = ctx.state.block_terminated;
        ctx.state.block_terminated = false;

        for statement in &node.node.block.node.statements {
            lower_node(statement, ctx)?;
            if ctx.state.block_terminated {
                break;
            }
        }

        let block_terminated = ctx.state.block_terminated;
        ctx.state.locals = saved_locals;
        ctx.state.block_terminated = prior_terminated || block_terminated;
        Ok(None)
    }
}
