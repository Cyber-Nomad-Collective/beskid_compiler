use crate::errors::CodegenError;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_analysis::hir::{HirIndexExpression, HirPrimitiveType};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeInfo;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, TrapCode, Value};

impl Lowerable<NodeLoweringContext<'_, '_>> for HirIndexExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        let target_type = ctx.require_expr_type_for_node(&node.node.target)?;
        let handle = lower_node(&node.node.target, ctx)?.ok_or(CodegenError::UnsupportedNode {
            span: node.node.target.span,
            node: "unit-valued index target",
        })?;
        let index = lower_node(&node.node.index, ctx)?.ok_or(CodegenError::UnsupportedNode {
            span: node.node.index.span,
            node: "unit-valued index",
        })?;

        match ctx.type_result.types.get(target_type) {
            Some(TypeInfo::Array(elem_type)) => {
                lower_array_read(node.span, handle, index, *elem_type, ctx)
            }
            Some(TypeInfo::Primitive(HirPrimitiveType::String)) => {
                lower_string_byte_read(node.span, handle, index, ctx)
            }
            _ => Err(CodegenError::UnsupportedNode {
                span: node.span,
                node: "index target type (expected array or string)",
            }),
        }
    }
}

/// Lower `arr[i]` — array element read through inline CLIF.
fn lower_array_read(
    span: beskid_analysis::syntax::SpanInfo,
    array_handle: Value,
    index: Value,
    elem_type: beskid_analysis::types::TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    // Load array.ptr (offset 0) and array.len (offset 8)
    let ptr = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), array_handle, 0);
    let len = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), array_handle, 8);

    // Bounds check: trap if index >= len
    let out_of_bounds = ctx
        .builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, len);
    ctx.builder
        .ins()
        .trapnz(out_of_bounds, TrapCode::unwrap_user(2));

    // Compute element size
    let layout = ctx.codegen.type_layout(ctx.type_result, elem_type).ok_or(
        CodegenError::UnsupportedNode {
            span,
            node: "array element layout",
        },
    )?;
    let elem_size_val = ctx.builder.ins().iconst(pointer_type(), layout.size as i64);

    // Compute address: ptr + index * elem_size
    let offset = ctx.builder.ins().imul(index, elem_size_val);
    let addr = ctx.builder.ins().iadd(ptr, offset);

    // Load element value at address
    let clif_ty =
        map_type_id_to_clif(ctx.type_result, elem_type).ok_or(CodegenError::UnsupportedNode {
            span,
            node: "array element clif type",
        })?;
    let value = ctx.builder.ins().load(clif_ty, MemFlags::new(), addr, 0);

    Ok(Some(value))
}

/// Lower `str[i]` — string byte read through inline CLIF.
fn lower_string_byte_read(
    _span: beskid_analysis::syntax::SpanInfo,
    str_handle: Value,
    index: Value,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    // Load str.ptr (offset 0) and str.len (offset 8)
    let ptr = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), str_handle, 0);
    let len = ctx
        .builder
        .ins()
        .load(pointer_type(), MemFlags::new(), str_handle, 8);

    // Bounds check: trap if index >= len
    let out_of_bounds = ctx
        .builder
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, index, len);
    ctx.builder
        .ins()
        .trapnz(out_of_bounds, TrapCode::unwrap_user(2));

    // Compute address: ptr + index (element_size = 1 for u8)
    let addr = ctx.builder.ins().iadd(ptr, index);

    // Load u8 byte from address
    let value = ctx
        .builder
        .ins()
        .load(cranelift_codegen::ir::types::I8, MemFlags::new(), addr, 0);

    Ok(Some(value))
}
