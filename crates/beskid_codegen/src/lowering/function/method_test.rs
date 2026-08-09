use std::collections::HashMap;

use beskid_analysis::{
    hir::{HirFunctionDefinition, HirMethodDefinition, HirTestDefinition},
    resolve::{ItemId, Resolution, canonical_item_id},
    syntax::Spanned,
    types::{TypeInfo, TypeResult},
};
use cranelift_codegen::{
    ir::{AbiParam, Function, InstBuilder, Signature},
    isa::CallConv,
    settings, verify_function,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use crate::{
    errors::CodegenError,
    lowering::{
        context::{CodegenContext, CodegenResult, LoweredFunction},
        locals::local_id_for_span,
        lowerable::lower_node,
        node_context::NodeLoweringContext,
        types::{map_type_id_to_clif, method_receiver_type_id, type_id_for_type},
    },
};

use super::{
    body_emission::{FunctionLoweringState, finish_emitting},
    mangling::{mangle_item_function, mangle_method_name},
    return_types::{resolve_return_type_id, signature_has_return},
};

pub(crate) fn lower_method(
    def: &Spanned<HirMethodDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    known_item_id: ItemId,
) -> CodegenResult<()> {
    let item_id = canonical_item_id(resolution, known_item_id);
    if ctx.current_source_path.is_none() {
        ctx.current_source_path = resolution.items.get(item_id.0).and_then(|info| info.source_path.clone());
    }
    ctx.emitting_items.insert(item_id);
    let result = lower_method_body(def, resolution, type_result, function_defs, ctx, item_id);
    finish_emitting(ctx, Some(item_id));
    result
}

fn lower_method_body(
    def: &Spanned<HirMethodDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    item_id: ItemId,
) -> CodegenResult<()> {
    let signature_types =
        type_result.function_signatures.get(&item_id).or_else(|| type_result.method_function_signatures.get(&item_id));

    let receiver_type_id = method_receiver_type_id(resolution, type_result, &def.node.receiver_type, item_id)
        .ok_or(CodegenError::UnsupportedNode { span: def.node.receiver_type.span, node: "method receiver type" })?;
    let receiver_clif_ty = map_type_id_to_clif(type_result, receiver_type_id)
        .ok_or(CodegenError::UnsupportedNode { span: def.node.receiver_type.span, node: "method receiver type" })?;

    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(receiver_clif_ty));
    for (index, param) in def.node.parameters.iter().enumerate() {
        let type_id = signature_types
            .and_then(|sig| sig.params.get(index).copied())
            .or_else(|| type_id_for_type(resolution, type_result, ctx.current_source_path.as_ref(), &param.node.ty))
            .ok_or(CodegenError::UnsupportedNode { span: param.span, node: "function parameter type" })?;
        let clif_ty = map_type_id_to_clif(type_result, type_id)
            .ok_or(CodegenError::UnsupportedNode { span: param.span, node: "function parameter type" })?;
        signature.params.push(AbiParam::new(clif_ty));
    }

    let return_type_id = resolve_return_type_id(
        resolution,
        type_result,
        ctx.current_source_path.as_ref(),
        def.node.return_type.as_ref(),
        signature_types.map(|sig| sig.return_type),
    );
    if let Some(type_id) = return_type_id
        && let Some(clif_ty) = map_type_id_to_clif(type_result, type_id)
    {
        signature.returns.push(AbiParam::new(clif_ty));
    }
    let expects_return = signature_has_return(&signature);

    let mut function = Function::new();
    function.signature = signature;

    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut fb_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let mut state = FunctionLoweringState::default();
    let param_values = builder.block_params(entry).to_vec();

    let this_local_id = local_id_for_span(resolution, def.node.receiver_type.span, ctx.current_source_path.as_ref())
        .ok_or(CodegenError::InvalidLocalBinding { span: def.node.receiver_type.span })?;
    let this_var = builder.declare_var(receiver_clif_ty);
    builder.def_var(this_var, param_values[0]);
    state.locals.insert(this_local_id, this_var);
    state.parameter_locals.push(this_local_id);
    state.local_type_overrides.insert(this_local_id, receiver_type_id);

    for (index, (param, value)) in def.node.parameters.iter().zip(param_values.iter().skip(1)).enumerate() {
        let local_id = local_id_for_span(resolution, param.node.name.span, ctx.current_source_path.as_ref())
            .ok_or(CodegenError::InvalidLocalBinding { span: param.node.name.span })?;
        let type_id = type_result
            .local_types
            .get(&local_id)
            .copied()
            .or_else(|| signature_types.and_then(|sig| sig.params.get(index).copied()))
            .or_else(|| type_id_for_type(resolution, type_result, ctx.current_source_path.as_ref(), &param.node.ty))
            .ok_or(CodegenError::MissingLocalType { span: param.node.name.span })?;
        let clif_ty = map_type_id_to_clif(type_result, type_id)
            .ok_or(CodegenError::UnsupportedNode { span: param.node.name.span, node: "function parameter type" })?;
        let var = builder.declare_var(clif_ty);
        builder.def_var(var, *value);
        state.locals.insert(local_id, var);
        state.parameter_locals.push(local_id);
        state.local_type_overrides.insert(local_id, type_id);
    }

    let mut node_ctx = NodeLoweringContext {
        resolution,
        type_result,
        codegen: ctx,
        function_defs,
        builder: &mut builder,
        state: &mut state,
        expected_return_type: return_type_id,
        expected_expr_type: None,
    };

    for statement in &def.node.body.node.statements {
        lower_node(statement, &mut node_ctx)?;
        if node_ctx.state.block_terminated {
            break;
        }
    }

    if !node_ctx.state.return_emitted && !node_ctx.state.block_terminated {
        if expects_return {
            return Err(CodegenError::UnsupportedNode { span: def.span, node: "implicit non-unit return" });
        }
        node_ctx.builder.ins().return_(&[]);
    }

    drop(node_ctx);
    builder.finalize();

    let receiver_item = match type_result.types.get(receiver_type_id) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => {
            return Err(CodegenError::UnsupportedNode {
                span: def.node.receiver_type.span,
                node: "method receiver item",
            });
        }
    };
    let receiver_name = resolution
        .items
        .iter()
        .find(|info| info.id == receiver_item)
        .map(|info| info.name.clone())
        .ok_or(CodegenError::MissingSymbol("method receiver item"))?;
    let function_name = mangle_method_name(&receiver_name, &def.node.name.node.name);

    let flags = settings::Flags::new(settings::builder());
    if let Err(err) = verify_function(&function, &flags) {
        return Err(CodegenError::VerificationFailed { function: function_name.clone(), message: err.to_string() });
    }

    ctx.functions_emitted += 1;
    ctx.lowered_functions.push(LoweredFunction { name: function_name, function });
    Ok(())
}

pub(crate) fn lower_test(
    def: &Spanned<HirTestDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
) -> CodegenResult<()> {
    let item_id = resolution.items.iter().find(|info| info.span == def.span).map(|info| info.id);
    let saved_source_path = ctx.current_source_path.clone();
    ctx.current_source_path = item_id
        .and_then(|id| resolution.items.get(id.0))
        .and_then(|info| info.source_path.clone())
        .or(saved_source_path.clone());
    let result = lower_test_body(def, resolution, type_result, function_defs, ctx, item_id);
    ctx.current_source_path = saved_source_path;
    result
}

fn lower_test_body(
    def: &Spanned<HirTestDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    item_id: Option<ItemId>,
) -> CodegenResult<()> {
    let signature_types = item_id.and_then(|id| type_result.function_signatures.get(&id));

    let mut signature = Signature::new(CallConv::SystemV);
    let return_type_id = signature_types.map(|sig| sig.return_type);
    if let Some(type_id) = return_type_id
        && let Some(clif_ty) = map_type_id_to_clif(type_result, type_id)
    {
        signature.returns.push(AbiParam::new(clif_ty));
    }
    let expects_return = signature_has_return(&signature);
    let expected_return_type = return_type_id;

    let mut function = Function::new();
    function.signature = signature;

    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut fb_ctx);
    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let mut state = FunctionLoweringState::default();
    let mut node_ctx = NodeLoweringContext {
        resolution,
        type_result,
        codegen: ctx,
        function_defs,
        builder: &mut builder,
        state: &mut state,
        expected_return_type,
        expected_expr_type: None,
    };

    for statement in &def.node.body.node.statements {
        lower_node(statement, &mut node_ctx)?;
        if node_ctx.state.block_terminated {
            break;
        }
    }

    if !node_ctx.state.return_emitted && !node_ctx.state.block_terminated {
        if expects_return {
            return Err(CodegenError::UnsupportedNode { span: def.span, node: "implicit non-unit return" });
        }
        node_ctx.builder.ins().return_(&[]);
    }

    drop(node_ctx);
    builder.finalize();

    let function_name =
        item_id.map(|id| mangle_item_function(resolution, id)).unwrap_or_else(|| def.node.name.node.name.clone());
    let flags = settings::Flags::new(settings::builder());
    if let Err(err) = verify_function(&function, &flags) {
        return Err(CodegenError::VerificationFailed { function: function_name.clone(), message: err.to_string() });
    }

    ctx.functions_emitted += 1;
    ctx.lowered_functions.push(LoweredFunction { name: function_name, function });
    Ok(())
}
