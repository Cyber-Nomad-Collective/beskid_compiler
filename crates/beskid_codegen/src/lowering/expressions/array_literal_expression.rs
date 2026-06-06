use crate::errors::CodegenError;
use crate::lowering::descriptor::is_pointer_like_type;
use crate::lowering::lowerable::{Lowerable, lower_node};
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_analysis::hir::HirArrayLiteralExpression;
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeInfo;
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, InstBuilder, MemFlags, Signature, Value,
};
use cranelift_codegen::isa::CallConv;

impl Lowerable<NodeLoweringContext<'_, '_>> for HirArrayLiteralExpression {
    type Output = Option<Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        let type_id = ctx.require_expr_type(node.span)?;
        let elem_type = match ctx.type_result.types.get(type_id) {
            Some(TypeInfo::Array(elem)) => *elem,
            _ => {
                return Err(CodegenError::UnsupportedNode {
                    span: node.span,
                    node: "array literal type (expected array)",
                });
            }
        };

        // Compute element size
        let layout = ctx.codegen.type_layout(ctx.type_result, elem_type).ok_or(
            CodegenError::UnsupportedNode {
                span: node.span,
                node: "array element layout",
            },
        )?;
        let elem_size = layout.size;
        let count = node.node.elements.len();

        // Call __array_new(elem_size, count) to allocate backing storage
        let array_ptr = emit_array_new(ctx, node.span, elem_size, count)?;

        // If there are no elements, just return the array handle
        if count == 0 {
            return Ok(Some(array_ptr));
        }

        // Load array.ptr (offset 0 of BeskidArray header)
        let data_ptr = ctx
            .builder
            .ins()
            .load(pointer_type(), MemFlags::new(), array_ptr, 0);

        // Determine if element type is pointer-like (requires GC write barrier)
        let needs_barrier = is_pointer_like_type(ctx.type_result, elem_type);

        // Store each element
        for (i, element) in node.node.elements.iter().enumerate() {
            let value = lower_node(element, ctx)?.ok_or(CodegenError::UnsupportedNode {
                span: element.span,
                node: "unit-valued array element",
            })?;

            let offset = ctx
                .builder
                .ins()
                .iconst(pointer_type(), (i * elem_size) as i64);
            let addr = ctx.builder.ins().iadd(data_ptr, offset);

            if needs_barrier {
                emit_write_barrier(ctx, array_ptr, value)?;
            }

            ctx.builder.ins().store(MemFlags::new(), value, addr, 0);
        }

        Ok(Some(array_ptr))
    }
}

/// Emit a call to the runtime `__array_new(elem_size: usize, len: usize) -> *mut BeskidArray`.
fn emit_array_new(
    ctx: &mut NodeLoweringContext<'_, '_>,
    span: beskid_analysis::syntax::SpanInfo,
    elem_size: usize,
    count: usize,
) -> Result<Value, CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase("array_new"),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });

    let elem_size_val = ctx.builder.ins().iconst(pointer_type(), elem_size as i64);
    let count_val = ctx.builder.ins().iconst(pointer_type(), count as i64);

    let call = ctx
        .builder
        .ins()
        .call(func_ref, &[elem_size_val, count_val]);
    let result =
        ctx.builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or(CodegenError::UnsupportedNode {
                span,
                node: "array_new result",
            })?;
    Ok(result)
}

/// Emit a GC write barrier call for pointer-like array elements.
fn emit_write_barrier(
    ctx: &mut NodeLoweringContext<'_, '_>,
    dst_obj: Value,
    value_ptr: Value,
) -> Result<(), CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.params.push(AbiParam::new(pointer_type()));
    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase("gc_write_barrier"),
        signature: sig_ref,
        colocated: false,
        patchable: false,
    });
    ctx.builder.ins().call(func_ref, &[dst_obj, value_ptr]);
    Ok(())
}
