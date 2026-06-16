use crate::errors::CodegenError;
use crate::lowering::descriptor::{struct_field_offsets, struct_item_id};
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::locals::infer_expr_type;
use crate::lowering::types::{is_fiber_handle_type, map_type_id_to_clif, pointer_type, resolve_monomorph_type_id};
use beskid_analysis::hir::HirMemberExpression;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value};

impl Lowerable<NodeLoweringContext<'_, '_>> for HirMemberExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        let target_value =
            lower_node(&node.node.target, ctx)?.ok_or(CodegenError::UnsupportedNode {
                span: node.node.target.span,
                node: "unit-valued member target",
            })?;
        let mut target_type = ctx.require_expr_type(node.node.target.span)?;
        target_type = resolve_monomorph_type_id(
            ctx.type_result,
            &ctx.codegen.active_generic_substitution,
            target_type,
        );
        if struct_item_id(ctx.type_result, target_type).is_none()
            && let Some(inferred) = infer_expr_type(
                ctx.resolution,
                ctx.type_result,
                &node.node.target,
                ctx.codegen.current_source_path.as_ref(),
                ctx.receiver_type,
            )
        {
            target_type = resolve_monomorph_type_id(
                ctx.type_result,
                &ctx.codegen.active_generic_substitution,
                inferred,
            );
        }
        let field_name = node.node.member.node.name.as_str();
        if field_name == "handle"
            && is_fiber_handle_type(ctx.type_result, ctx.resolution, target_type)
        {
            return Ok(Some(target_value));
        }
        let item_id =
            struct_item_id(ctx.type_result, target_type).ok_or(CodegenError::UnsupportedNode {
                span: node.node.target.span,
                node: "member target type",
            })?;
        let offsets = struct_field_offsets(
            ctx.resolution,
            ctx.type_result,
            item_id,
            ctx.codegen.current_source_path.as_ref(),
        )
        .ok_or(
            CodegenError::UnsupportedNode {
                span: node.span,
                node: "member offsets",
            },
        )?;
        let offset = offsets
            .get(field_name)
            .copied()
            .ok_or(CodegenError::UnsupportedNode {
                span: node.node.member.span,
                node: "member offset",
            })?;
        let field_type = ctx
            .type_result
            .struct_fields_ordered
            .get(&item_id)
            .and_then(|fields| fields.iter().find(|(name, _)| name == field_name))
            .map(|(_, ty)| *ty)
            .ok_or(CodegenError::UnsupportedNode {
                span: node.node.member.span,
                node: "member field type",
            })?;
        let clif_ty = map_type_id_to_clif(ctx.type_result, field_type).ok_or(
            CodegenError::UnsupportedNode {
                span: node.node.member.span,
                node: "member field clif type",
            },
        )?;
        let offset_val = ctx.builder.ins().iconst(pointer_type(), offset as i64);
        let addr = ctx.builder.ins().iadd(target_value, offset_val);
        let value = ctx.builder.ins().load(clif_ty, MemFlags::new(), addr, 0);
        Ok(Some(value))
    }
}
