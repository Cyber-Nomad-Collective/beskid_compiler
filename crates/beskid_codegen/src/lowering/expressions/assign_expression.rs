use crate::errors::CodegenError;
use crate::lowering::cast_intent::ensure_type_compatibility;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use beskid_analysis::hir::{HirAssignExpression, HirAssignOp, HirExpressionNode};
use beskid_analysis::resolve::ResolvedValue;
use beskid_analysis::types::TypeInfo;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::Value;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirAssignExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        let HirExpressionNode::PathExpression(path_expr) = &node.node.target.node else {
            return Err(CodegenError::UnsupportedNode {
                span: node.node.target.span,
                node: "non-path assignment target",
            });
        };
        if path_expr.node.path.node.segments.len() != 1 {
            return Err(CodegenError::UnsupportedNode {
                span: node.node.target.span,
                node: "multi-segment assignment target",
            });
        }

        let resolved = ctx
            .resolution
            .tables
            .resolved_values
            .get(&path_expr.node.path.span)
            .ok_or(CodegenError::MissingResolvedValue {
                span: path_expr.node.path.span,
            })?;
        let ResolvedValue::Local(local_id) = resolved else {
            return Err(CodegenError::UnsupportedNode {
                span: path_expr.node.path.span,
                node: "non-local assignment target",
            });
        };
        let var =
            ctx.state
                .locals
                .get(local_id)
                .copied()
                .ok_or(CodegenError::InvalidLocalBinding {
                    span: path_expr.node.path.span,
                })?;

        let value = lower_node(&node.node.value, ctx)?.ok_or(CodegenError::UnsupportedNode {
            span: node.node.value.span,
            node: "unit-valued assignment",
        })?;

        let expected_type = ctx.type_result.local_types.get(local_id).copied().ok_or(
            CodegenError::MissingLocalType {
                span: path_expr.node.path.span,
            },
        )?;
        let actual_type = ctx
            .type_result
            .expr_types
            .get(&node.node.value.span)
            .copied()
            .ok_or(CodegenError::MissingExpressionType {
                span: node.node.value.span,
            })?;
        let value = ensure_type_compatibility(
            node.node.value.span,
            expected_type,
            actual_type,
            ctx.type_result,
            ctx.builder,
            value,
        )?;

        let assigned = match node.node.op.node {
            HirAssignOp::Assign => value,
            HirAssignOp::AddAssign | HirAssignOp::SubAssign => {
                let current = ctx.builder.use_var(var);
                let is_float = matches!(
                    ctx.type_result.types.get(expected_type),
                    Some(TypeInfo::Primitive(beskid_analysis::hir::HirPrimitiveType::F64))
                );
                if is_float {
                    match node.node.op.node {
                        HirAssignOp::AddAssign => ctx.builder.ins().fadd(current, value),
                        HirAssignOp::SubAssign => ctx.builder.ins().fsub(current, value),
                        HirAssignOp::Assign => unreachable!("handled above"),
                    }
                } else {
                    match node.node.op.node {
                        HirAssignOp::AddAssign => ctx.builder.ins().iadd(current, value),
                        HirAssignOp::SubAssign => ctx.builder.ins().isub(current, value),
                        HirAssignOp::Assign => unreachable!("handled above"),
                    }
                }
            }
        };

        ctx.builder.def_var(var, assigned);
        Ok(Some(assigned))
    }
}
