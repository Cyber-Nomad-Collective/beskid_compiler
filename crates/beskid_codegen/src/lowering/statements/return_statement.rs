use crate::errors::CodegenError;
use crate::lowering::cast_intent::ensure_type_compatibility_or_expected;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use beskid_analysis::hir::HirReturnStatement;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::InstBuilder;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirReturnStatement {
    type Output = ();

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        match &node.node.value {
            Some(value_expr) => {
                // Do not thread `expected_return_type` through nested expression lowering: it poisons
                // `require_expr_type_for_node` for call arguments (e.g. string literals passed to
                // `__str_len`) and triggers bogus numeric→string coercion on pointer handles.
                let lowered = lower_node(value_expr, ctx)?;
                match lowered {
                    Some(mut value) => {
                        if let Some(expected) = ctx.expected_return_type {
                            // Dependency bodies may lack full expression typing; infer when possible
                            // so CLIF coercion (e.g. i32→i64) still runs against the lowered value.
                            let actual = ctx
                                .require_expr_type_for_node(value_expr)
                                .unwrap_or(expected);
                            value = ensure_type_compatibility_or_expected(
                                value_expr.span,
                                expected,
                                actual,
                                ctx.type_result,
                                ctx.resolution,
                                ctx.builder,
                                value,
                            )?;
                        }
                        ctx.builder.ins().return_(&[value]);
                    }
                    None => {
                        if ctx.builder.func.signature.returns.is_empty() {
                            ctx.builder.ins().return_(&[]);
                        } else {
                            return Err(CodegenError::UnsupportedNode {
                                span: value_expr.span,
                                node: "missing return value",
                            });
                        }
                    }
                }
            }
            None => {
                ctx.builder.ins().return_(&[]);
            }
        }

        ctx.state.return_emitted = true;
        ctx.state.block_terminated = true;
        Ok(())
    }
}
