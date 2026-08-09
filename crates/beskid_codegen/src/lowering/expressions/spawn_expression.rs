use crate::errors::CodegenError;
use crate::lowering::context::LoweredFunction;
use crate::lowering::dispatch::lower_dispatch_builtin_call;
use crate::lowering::expressions::call_expression::{lower_spawn_lambda_target, type_returns_runtime_value};
use crate::lowering::function::{linker_name_for_item_function, lower_function_with_name, mangle_item_function};
use crate::lowering::locals::resolved_value_at;
use crate::lowering::lowerable::Lowerable;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::{map_type_id_to_clif, pointer_type};
use beskid_abi::{DispatchReturnGroup, DispatchRoute, TAG_FIBER_SPAWN_WITH_CANCEL_SLOT};
use beskid_analysis::hir::{HirExpressionNode, HirSpawnExpression};
use beskid_analysis::resolve::{ItemId, ResolvedValue, canonical_item_id};
use beskid_analysis::syntax::{SpanInfo, Spanned};
use cranelift_codegen::ir::{
    AbiParam, ExtFuncData, ExternalName, Function, InstBuilder, Signature, StackSlotData, StackSlotKind, Value, types,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

/// Lower `spawn callee` to `fiber_spawn_with_cancel_slot(entry_trampoline, env, event_slot)`.
impl Lowerable<NodeLoweringContext<'_, '_>> for HirSpawnExpression {
    type Output = Option<cranelift_codegen::ir::Value>;

    fn lower(node: &Spanned<Self>, ctx: &mut NodeLoweringContext<'_, '_>) -> Result<Self::Output, CodegenError> {
        lower_spawn_expression(node, ctx)
    }
}

fn fiber_entry_signature() -> Signature {
    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(pointer_type()));
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn emit_fiber_entry_trampoline(
    target_name: &str,
    target_sig: Signature,
    returns_value: bool,
    spawn_span: SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    if !target_sig.params.is_empty() {
        return Err(CodegenError::UnsupportedNode {
            span: spawn_span,
            node: "spawn target function parameters",
        });
    }
    let trampoline_name =
        format!("__beskid_spawn_entry_{}", ctx.codegen.functions_emitted + ctx.codegen.lowered_functions.len());

    let mut function = Function::new();
    function.signature = fiber_entry_signature();
    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut fb_ctx);

    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);
    builder.seal_block(entry_block);

    let sig_ref = builder.func.import_signature(target_sig);
    let func_ref = builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(target_name.as_bytes()),
        signature: sig_ref,
        colocated: true,
        patchable: false,
    });
    let call = builder.ins().call(func_ref, &[]);
    let return_value = if returns_value {
        *builder.inst_results(call).first().ok_or(CodegenError::UnsupportedNode {
            span: Default::default(),
            node: "spawn entry trampoline call result",
        })?
    } else {
        builder.ins().iconst(types::I64, 0)
    };
    builder.ins().return_(&[return_value]);
    builder.finalize();

    let flags = settings::Flags::new(settings::builder());
    if let Err(err) = verify_function(&function, &flags) {
        return Err(CodegenError::VerificationFailed { function: trampoline_name.clone(), message: err.to_string() });
    }

    ctx.codegen.functions_emitted += 1;
    ctx.codegen.lowered_functions.push(LoweredFunction { name: trampoline_name.clone(), function });

    let sig_ref = ctx.builder.func.import_signature(fiber_entry_signature());
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(trampoline_name.as_bytes()),
        signature: sig_ref,
        colocated: true,
        patchable: false,
    });
    Ok(ctx.builder.ins().func_addr(pointer_type(), func_ref))
}

fn function_signature_for_item(
    item_id: ItemId,
    ctx: &NodeLoweringContext<'_, '_>,
    span: SpanInfo,
) -> Result<(Signature, bool), CodegenError> {
    let signature_types = ctx
        .type_result
        .function_signatures
        .get(&item_id)
        .ok_or(CodegenError::MissingSymbol("spawn entry signature"))?;
    let mut signature = Signature::new(CallConv::SystemV);
    for param in &signature_types.params {
        let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
            .ok_or(CodegenError::UnsupportedNode { span, node: "spawn entry parameter type" })?;
        signature.params.push(AbiParam::new(clif_ty));
    }
    let returns_value = type_returns_runtime_value(ctx.type_result, signature_types.return_type);
    if returns_value {
        let clif_ty = map_type_id_to_clif(ctx.type_result, signature_types.return_type)
            .ok_or(CodegenError::UnsupportedNode { span, node: "spawn entry return type" })?;
        signature.returns.push(AbiParam::new(clif_ty));
    }
    Ok((signature, returns_value))
}

fn ensure_spawn_path_target_emitted(
    item_id: ItemId,
    symbol_name: &str,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(), CodegenError> {
    if ctx.codegen.symbol_emitted(symbol_name) || ctx.codegen.emitting_items.contains(&item_id) {
        return Ok(());
    }
    let def = ctx.function_defs.get(&item_id).ok_or(CodegenError::MissingSymbol("spawn entry function definition"))?;
    let saved_source_path = ctx.codegen.current_source_path.clone();
    ctx.codegen.current_source_path = ctx
        .resolution
        .items
        .get(item_id.0)
        .and_then(|info| info.source_path.clone())
        .or_else(|| saved_source_path.clone());
    let lower_result = lower_function_with_name(
        def,
        ctx.resolution,
        ctx.type_result,
        ctx.function_defs,
        ctx.codegen,
        None,
        None,
        Some(item_id),
    );
    ctx.codegen.current_source_path = saved_source_path;
    lower_result
}

fn resolve_spawn_path_target(
    item_id: ItemId,
    span: SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(String, Signature, bool), CodegenError> {
    let item_id = canonical_item_id(ctx.resolution, item_id);
    if ctx.resolution.items.get(item_id.0).is_none() {
        return Err(CodegenError::MissingSymbol("spawn entry function"));
    }
    let symbol_name = ctx
        .function_defs
        .get(&item_id)
        .map(|def| linker_name_for_item_function(ctx.resolution, item_id, def))
        .unwrap_or_else(|| mangle_item_function(ctx.resolution, item_id));
    ensure_spawn_path_target_emitted(item_id, &symbol_name, ctx)?;
    if !ctx.codegen.symbol_emitted(&symbol_name) {
        return Err(CodegenError::VerificationFailed {
            function: symbol_name.clone(),
            message: "spawn entry target missing from link plan".to_string(),
        });
    }
    let (signature, returns_value) = function_signature_for_item(item_id, ctx, span)?;
    Ok((symbol_name, signature, returns_value))
}

fn lower_spawn_entry(
    callee: &Spanned<HirExpressionNode>,
    spawn_span: SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    let (target_name, target_sig, returns_value) = match &callee.node {
        HirExpressionNode::LambdaExpression(lambda) => lower_spawn_lambda_target(lambda, spawn_span, ctx)?,
        HirExpressionNode::CallExpression(call) if call.node.args.is_empty() => {
            return lower_spawn_entry(&call.node.callee, spawn_span, ctx);
        }
        HirExpressionNode::CallExpression(_) => {
            return Err(CodegenError::UnsupportedNode { span: spawn_span, node: "spawn callee arguments" });
        }
        HirExpressionNode::PathExpression(path) => {
            let item_id =
                resolved_value_at(ctx.resolution, path.node.path.span, ctx.codegen.current_source_path.as_ref())
                    .and_then(|resolved| match resolved {
                        ResolvedValue::Item(item_id) => Some(item_id),
                        _ => None,
                    })
                    .ok_or(CodegenError::UnsupportedNode { span: spawn_span, node: "spawn entry path" })?;
            resolve_spawn_path_target(item_id, spawn_span, ctx)?
        }
        _ => {
            return Err(CodegenError::UnsupportedNode { span: spawn_span, node: "spawn callee" });
        }
    };
    emit_fiber_entry_trampoline(&target_name, target_sig, returns_value, spawn_span, ctx)
}

fn lower_spawn_expression(
    spawn: &Spanned<HirSpawnExpression>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    let entry_ptr = lower_spawn_entry(&spawn.node.callee, spawn.span, ctx)?;

    let env = ctx.builder.ins().iconst(pointer_type(), 0);
    let on_cancelled_slot = ctx.builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    ctx.builder.ins().stack_store(env, on_cancelled_slot, 0);
    let on_cancelled_slot_addr = ctx.builder.ins().stack_addr(pointer_type(), on_cancelled_slot, 0);

    let handle = lower_dispatch_builtin_call(
        spawn.span,
        DispatchRoute { tag: TAG_FIBER_SPAWN_WITH_CANCEL_SLOT, group: DispatchReturnGroup::I64 },
        &[entry_ptr, env, on_cancelled_slot_addr],
        true,
        ctx,
    )?
    .ok_or(CodegenError::UnsupportedNode { span: spawn.span, node: "fiber_spawn_with_cancel_slot result" })?;
    Ok(Some(handle))
}
