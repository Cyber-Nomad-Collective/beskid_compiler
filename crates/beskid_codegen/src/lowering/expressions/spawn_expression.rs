use crate::errors::CodegenError;
use crate::lowering::dispatch::lower_dispatch_builtin_call;
use crate::lowering::expressions::call_expression::lower_lambda_function_value;
use crate::lowering::lowerable::Lowerable;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::pointer_type;
use beskid_abi::{DispatchReturnGroup, DispatchRoute, TAG_FIBER_SPAWN_WITH_CANCEL_SLOT};
use beskid_analysis::hir::{HirExpressionNode, HirSpawnExpression};
use beskid_analysis::resolve::ResolvedValue;
use beskid_analysis::syntax::Spanned;
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, InstBuilder, Signature, StackSlotData, StackSlotKind,
};
use cranelift_codegen::isa::CallConv;

/// Lower `spawn callee` to `fiber_spawn_with_cancel_slot(entry_trampoline, env, event_slot)`.
impl Lowerable<NodeLoweringContext<'_, '_>> for HirSpawnExpression {
    type Output = Option<cranelift_codegen::ir::Value>;

    fn lower(
        node: &Spanned<Self>,
        ctx: &mut NodeLoweringContext<'_, '_>,
    ) -> Result<Self::Output, CodegenError> {
        lower_spawn_expression(node, ctx)
    }
}

fn lower_spawn_expression(
    spawn: &Spanned<HirSpawnExpression>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<cranelift_codegen::ir::Value>, CodegenError> {
    let entry_ptr = lower_spawn_entry(&spawn.node.callee, spawn.span, ctx)?;

    let env = ctx.builder.ins().iconst(pointer_type(), 0);
    let on_cancelled_slot =
        ctx.builder
            .create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    ctx.builder.ins().stack_store(env, on_cancelled_slot, 0);
    let on_cancelled_slot_addr = ctx
        .builder
        .ins()
        .stack_addr(pointer_type(), on_cancelled_slot, 0);

    let handle = lower_dispatch_builtin_call(
        spawn.span,
        DispatchRoute {
            tag: TAG_FIBER_SPAWN_WITH_CANCEL_SLOT,
            group: DispatchReturnGroup::I64,
        },
        &[entry_ptr, env, on_cancelled_slot_addr],
        true,
        ctx,
    )?
    .ok_or(CodegenError::UnsupportedNode {
        span: spawn.span,
        node: "fiber_spawn_with_cancel_slot result",
    })?;
    Ok(Some(handle))
}

fn lower_spawn_entry(
    callee: &Spanned<HirExpressionNode>,
    spawn_span: beskid_analysis::syntax::SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<cranelift_codegen::ir::Value, CodegenError> {
    match &callee.node {
        HirExpressionNode::LambdaExpression(lambda) => {
            lower_lambda_function_value(lambda, spawn_span, ctx)
        }
        HirExpressionNode::CallExpression(call) if call.node.args.is_empty() => {
            lower_spawn_entry(&call.node.callee, spawn_span, ctx)
        }
        HirExpressionNode::CallExpression(_) => Err(CodegenError::UnsupportedNode {
            span: spawn_span,
            node: "spawn callee arguments",
        }),
        HirExpressionNode::PathExpression(path) => {
            if let Some(ResolvedValue::Item(item_id)) = ctx
                .resolution
                .tables
                .resolved_values
                .get(&path.node.path.span)
            {
                let name = ctx
                    .resolution
                    .items
                    .get(item_id.0)
                    .map(|i| i.name.clone())
                    .ok_or(CodegenError::MissingSymbol("spawn entry function"))?;
                let sig_ref = ctx
                    .builder
                    .func
                    .import_signature(Signature::new(CallConv::SystemV));
                let func_ref = ctx.builder.func.import_function(ExtFuncData {
                    name: ExternalName::testcase(name),
                    signature: sig_ref,
                    colocated: true,
                    patchable: false,
                });
                Ok(ctx.builder.ins().func_addr(pointer_type(), func_ref))
            } else {
                Err(CodegenError::UnsupportedNode {
                    span: spawn_span,
                    node: "spawn entry path",
                })
            }
        }
        _ => Err(CodegenError::UnsupportedNode {
            span: spawn_span,
            node: "spawn callee",
        }),
    }
}
