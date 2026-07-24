use crate::errors::CodegenError;
use crate::lowering::cast_intent::ensure_type_compatibility_or_expected;
use crate::lowering::locals::local_id_for_span;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::map_type_id_to_clif;
use beskid_analysis::hir::{HirExpressionNode, HirLetStatement};
use beskid_analysis::syntax::Spanned;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirLetStatement {
    type Output = ();

    fn lower(node: &Spanned<Self>, ctx: &mut NodeLoweringContext<'_, '_>) -> Result<Self::Output, CodegenError> {
        let local_id = local_id_for_span(ctx.resolution, node.node.name.span, ctx.codegen.current_source_path.as_ref())
            .ok_or(CodegenError::InvalidLocalBinding { span: node.node.name.span })?;

        // Prefer the written type annotation over span-keyed `local_types`, which can collide
        // across materialized compilation units in linked assemblies.
        let type_id = node
            .node
            .type_annotation
            .as_ref()
            .and_then(|ty| {
                crate::lowering::types::type_id_for_type(
                    ctx.resolution,
                    ctx.type_result,
                    ctx.codegen.current_source_path.as_ref(),
                    ty,
                )
            })
            .or_else(|| ctx.type_result.local_types.get(&local_id).copied())
            .or_else(|| ctx.require_expr_type_for_node(&node.node.value).ok())
            .ok_or(CodegenError::MissingLocalType { span: node.node.name.span })?;
        let clif_ty = map_type_id_to_clif(ctx.type_result, type_id)
            .or_else(|| ctx.type_result.types.get(type_id).map(|_| crate::lowering::types::pointer_type()))
            .ok_or(CodegenError::UnsupportedNode { span: node.node.name.span, node: "unsupported local type" })?;

        if let HirExpressionNode::LambdaExpression(lambda) = &node.node.value.node {
            ctx.state.local_lambdas.insert(local_id, lambda as *const Spanned<_>);

            let lowered = match lower_node(&node.node.value, ctx) {
                Ok(value) => value,
                Err(CodegenError::InvalidLocalBinding { .. }) => {
                    // Capturing lambdas still flow through the inline local-lambda path.
                    return Ok(());
                }
                Err(err) => return Err(err),
            };

            let value = lowered.ok_or(CodegenError::UnsupportedNode {
                span: node.node.value.span,
                node: "unit-valued let initializer",
            })?;

            let actual_type = ctx.require_expr_type_for_node(&node.node.value).unwrap_or(type_id);
            let value = ensure_type_compatibility_or_expected(
                node.node.value.span,
                type_id,
                actual_type,
                ctx.type_result,
                ctx.resolution,
                ctx.builder,
                value,
            )?;

            let var = ctx.builder.declare_var(clif_ty);
            ctx.builder.def_var(var, value);
            ctx.state.locals.insert(local_id, var);
            ctx.state.local_type_overrides.insert(local_id, type_id);
            return Ok(());
        }

        let value = lower_node(&node.node.value, ctx)?
            .ok_or(CodegenError::UnsupportedNode { span: node.node.value.span, node: "unit-valued let initializer" })?;

        let actual_type = ctx.require_expr_type_for_node(&node.node.value).unwrap_or(type_id);
        let value = ensure_type_compatibility_or_expected(
            node.node.value.span,
            type_id,
            actual_type,
            ctx.type_result,
            ctx.resolution,
            ctx.builder,
            value,
        )?;

        let var = ctx.builder.declare_var(clif_ty);
        ctx.builder.def_var(var, value);
        ctx.state.locals.insert(local_id, var);
        ctx.state.local_type_overrides.insert(local_id, type_id);
        Ok(())
    }
}
