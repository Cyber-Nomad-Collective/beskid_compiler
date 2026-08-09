use std::collections::HashMap;

use beskid_analysis::{
    hir::{HirFunctionDefinition, HirLambdaExpression},
    resolve::{ItemId, LocalId, Resolution},
    syntax::Spanned,
    types::{TypeId, TypeResult},
};
use cranelift_codegen::{
    ir::{AbiParam, Block, Function, InstBuilder, Signature},
    isa::CallConv,
    settings, verify_function,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};

use crate::{
    errors::CodegenError,
    lowering::{
        context::{CodegenContext, CodegenResult, LoweredFunction},
        expressions::export::{export_linker_name, validate_export_function},
        locals::local_id_for_span,
        lowerable::lower_node,
        node_context::NodeLoweringContext,
        types::{map_type_id_to_clif, type_id_for_type},
    },
};

use super::{
    generics::substitute_type_id,
    mangling::linker_name_for_item_function,
    return_types::{resolve_return_type_id, signature_has_return},
};

pub(super) fn finish_emitting(ctx: &mut CodegenContext, item_id: Option<ItemId>) {
    if let Some(id) = item_id {
        ctx.emitting_items.remove(&id);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_function_with_name_body(
    def: &Spanned<HirFunctionDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    name_override: Option<String>,
    generic_args: &HashMap<String, beskid_analysis::types::TypeId>,
    item_id: Option<ItemId>,
) -> CodegenResult<()> {
    let substitute = |type_id: beskid_analysis::types::TypeId| -> beskid_analysis::types::TypeId {
        substitute_type_id(type_result, type_id, generic_args)
    };
    let signature_types = item_id.and_then(|id| type_result.function_signatures.get(&id));
    let mut signature = Signature::new(CallConv::SystemV);
    for (index, param) in def.node.parameters.iter().enumerate() {
        let type_id = signature_types
            .and_then(|sig| sig.params.get(index).copied())
            .or_else(|| type_id_for_type(resolution, type_result, ctx.current_source_path.as_ref(), &param.node.ty))
            .map(&substitute)
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
    )
    .map(&substitute);
    if let Some(type_id) = return_type_id
        && let Some(clif_ty) = map_type_id_to_clif(type_result, type_id)
    {
        signature.returns.push(AbiParam::new(clif_ty));
    }
    let expects_return = signature_has_return(&signature);
    let expected_return_type = return_type_id;

    let pointer = cranelift_codegen::ir::types::I64;
    let _export_entry = validate_export_function(def, &signature, pointer)?;

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
    for (index, (param, value)) in def.node.parameters.iter().zip(param_values).enumerate() {
        let local_id = local_id_for_span(resolution, param.node.name.span, ctx.current_source_path.as_ref())
            .ok_or(CodegenError::InvalidLocalBinding { span: param.node.name.span })?;
        let type_id = signature_types
            .and_then(|sig| sig.params.get(index).copied())
            .or_else(|| type_result.local_types.get(&local_id).copied())
            .or_else(|| type_id_for_type(resolution, type_result, ctx.current_source_path.as_ref(), &param.node.ty))
            .map(&substitute)
            .ok_or(CodegenError::MissingLocalType { span: param.node.name.span })?;
        let clif_ty = map_type_id_to_clif(type_result, type_id)
            .ok_or(CodegenError::UnsupportedNode { span: param.node.name.span, node: "function parameter type" })?;
        let var = builder.declare_var(clif_ty);
        builder.def_var(var, value);
        state.locals.insert(local_id, var);
        state.parameter_locals.push(local_id);
        state.local_type_overrides.insert(local_id, type_id);
        state.local_type_overrides.insert(local_id, type_id);
    }

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

    let flags = settings::Flags::new(settings::builder());
    if let Err(err) = verify_function(&function, &flags) {
        return Err(CodegenError::VerificationFailed {
            function: def.node.name.node.name.clone(),
            message: err.to_string(),
        });
    }

    ctx.functions_emitted += 1;
    let function_name = name_override.unwrap_or_else(|| {
        item_id.map(|id| linker_name_for_item_function(resolution, id, def)).unwrap_or_else(|| export_linker_name(def))
    });
    ctx.lowered_functions.push(LoweredFunction { name: function_name, function });

    Ok(())
}

#[derive(Default)]
pub(crate) struct FunctionLoweringState {
    pub(crate) locals: HashMap<LocalId, Variable>,
    pub(crate) parameter_locals: Vec<LocalId>,
    pub(crate) local_type_overrides: HashMap<LocalId, TypeId>,
    pub(crate) local_lambdas: HashMap<LocalId, *const Spanned<HirLambdaExpression>>,
    pub(crate) emitted_lambda_symbols: HashMap<*const Spanned<HirLambdaExpression>, String>,
    pub(crate) return_emitted: bool,
    pub(crate) block_terminated: bool,
    pub(crate) loop_stack: Vec<LoopControl>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopControl {
    pub(crate) continue_block: Block,
    pub(crate) break_block: Block,
}

/// Re-read parameter locals at the loop header so invariant bindings survive backedges.
pub(crate) fn refresh_locals_at_loop_header(builder: &mut FunctionBuilder, state: &FunctionLoweringState) {
    for local_id in &state.parameter_locals {
        let Some(var) = state.locals.get(local_id) else {
            continue;
        };
        let value = builder.use_var(*var);
        builder.def_var(*var, value);
    }
}
