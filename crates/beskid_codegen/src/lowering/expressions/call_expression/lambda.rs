use super::common::type_returns_runtime_value;
use super::{
    CodegenError, ExtFuncData, ExternalName, Function, FunctionBuilder, FunctionBuilderContext, HirCallExpression,
    HirExpressionNode, HirLambdaExpression, InstBuilder, NodeLoweringContext, ResolvedValue, Signature, SpanInfo,
    Spanned, TypeId, TypeInfo, Value, Variable, ensure_type_compatibility_or_expected, local_id_for_span, lower_node,
    map_type_id_to_clif, pointer_type, resolved_value_at, settings, verify_function,
};
use cranelift_codegen::ir::AbiParam;
use cranelift_codegen::isa::CallConv;

fn lambda_signature_type_ids(
    lambda: &Spanned<HirLambdaExpression>,
    ctx: &NodeLoweringContext<'_, '_>,
) -> Result<(Vec<TypeId>, TypeId), CodegenError> {
    let mut params = Vec::with_capacity(lambda.node.parameters.len());
    for parameter in &lambda.node.parameters {
        let local_id =
            local_id_for_span(ctx.resolution, parameter.node.name.span, ctx.codegen.current_source_path.as_ref())
                .ok_or(CodegenError::InvalidLocalBinding { span: parameter.node.name.span })?;
        let type_id = ctx
            .type_result
            .local_types
            .get(&local_id)
            .copied()
            .ok_or(CodegenError::MissingLocalType { span: parameter.node.name.span })?;
        params.push(type_id);
    }

    let return_type = ctx
        .type_result
        .node_type(lambda.node.body.id)
        .ok_or(CodegenError::MissingExpressionType { span: lambda.node.body.span })?;

    Ok((params, return_type))
}

pub(super) fn lambda_signature_from_types(
    params: &[TypeId],
    return_type: TypeId,
    span: beskid_analysis::syntax::SpanInfo,
    ctx: &NodeLoweringContext<'_, '_>,
) -> Result<(Signature, bool), CodegenError> {
    let mut signature = Signature::new(CallConv::SystemV);
    for param in params {
        let clif_ty = map_type_id_to_clif(ctx.type_result, *param)
            .ok_or(CodegenError::UnsupportedNode { span, node: "lambda parameter type" })?;
        signature.params.push(AbiParam::new(clif_ty));
    }
    let returns_value = type_returns_runtime_value(ctx.type_result, return_type);
    if returns_value {
        let clif_ty = map_type_id_to_clif(ctx.type_result, return_type)
            .ok_or(CodegenError::UnsupportedNode { span, node: "lambda return type" })?;
        signature.returns.push(AbiParam::new(clif_ty));
    }
    Ok((signature, returns_value))
}

pub(crate) fn lower_spawn_lambda_target(
    lambda: &Spanned<HirLambdaExpression>,
    span: SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<(String, Signature, bool), CodegenError> {
    let (param_types, return_type) = lambda_signature_type_ids(lambda, ctx)?;
    let (signature, returns_value) = lambda_signature_from_types(&param_types, return_type, span, ctx)?;
    let name = lower_lambda_to_symbol(lambda, ctx)?;
    Ok((name, signature, returns_value))
}

fn lower_lambda_to_symbol(
    lambda: &Spanned<HirLambdaExpression>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<String, CodegenError> {
    let lambda_key = lambda as *const Spanned<HirLambdaExpression>;
    if let Some(existing) = ctx.state.emitted_lambda_symbols.get(&lambda_key) {
        return Ok(existing.clone());
    }

    let (param_types, return_type) = lambda_signature_type_ids(lambda, ctx)?;
    let (signature, returns_value) = lambda_signature_from_types(&param_types, return_type, lambda.span, ctx)?;

    let name = format!("__beskid_lambda_{}", ctx.codegen.functions_emitted + ctx.codegen.lowered_functions.len());

    let mut function = Function::new();
    function.signature = signature.clone();
    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut fb_ctx);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let mut state = crate::lowering::function::FunctionLoweringState::default();
    let param_values = builder.block_params(entry).to_vec();
    for (parameter, value) in lambda.node.parameters.iter().zip(param_values) {
        let local_id =
            local_id_for_span(ctx.resolution, parameter.node.name.span, ctx.codegen.current_source_path.as_ref())
                .ok_or(CodegenError::InvalidLocalBinding { span: parameter.node.name.span })?;
        let type_id = ctx
            .type_result
            .local_types
            .get(&local_id)
            .copied()
            .ok_or(CodegenError::MissingLocalType { span: parameter.node.name.span })?;
        let clif_ty = map_type_id_to_clif(ctx.type_result, type_id)
            .ok_or(CodegenError::UnsupportedNode { span: parameter.node.name.span, node: "lambda parameter type" })?;
        let var = builder.declare_var(clif_ty);
        builder.def_var(var, value);
        state.locals.insert(local_id, var);
    }

    let mut lambda_ctx = NodeLoweringContext {
        resolution: ctx.resolution,
        type_result: ctx.type_result,
        codegen: ctx.codegen,
        function_defs: ctx.function_defs,
        builder: &mut builder,
        state: &mut state,
        expected_return_type: Some(return_type),
        expected_expr_type: None,
    };

    let lowered = lower_node(&lambda.node.body, &mut lambda_ctx)?;
    if !lambda_ctx.state.return_emitted && !lambda_ctx.state.block_terminated {
        if returns_value {
            let value = lowered.ok_or(CodegenError::UnsupportedNode {
                span: lambda.node.body.span,
                node: "unit-valued lambda body",
            })?;
            lambda_ctx.builder.ins().return_(&[value]);
        } else {
            lambda_ctx.builder.ins().return_(&[]);
        }
    }

    drop(lambda_ctx);
    builder.finalize();

    let flags = settings::Flags::new(settings::builder());
    if let Err(err) = verify_function(&function, &flags) {
        return Err(CodegenError::VerificationFailed { function: name.clone(), message: err.to_string() });
    }

    ctx.codegen.functions_emitted += 1;
    ctx.codegen.lowered_functions.push(crate::lowering::context::LoweredFunction { name: name.clone(), function });
    ctx.state.emitted_lambda_symbols.insert(lambda_key, name.clone());
    Ok(name)
}

pub(crate) fn lower_lambda_function_value(
    lambda: &Spanned<HirLambdaExpression>,
    span: beskid_analysis::syntax::SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    let (param_types, return_type) = lambda_signature_type_ids(lambda, ctx)?;
    let (signature, _) = lambda_signature_from_types(&param_types, return_type, span, ctx)?;
    let name = lower_lambda_to_symbol(lambda, ctx)?;

    let sig_ref = ctx.builder.func.import_signature(signature);
    let func_ref = ctx.builder.func.import_function(ExtFuncData {
        name: ExternalName::testcase(name),
        signature: sig_ref,
        colocated: true,
        patchable: false,
    });

    let _ = span;
    Ok(ctx.builder.ins().func_addr(pointer_type(), func_ref))
}

fn lower_lambda_function_value_checked(
    lambda: &Spanned<HirLambdaExpression>,
    span: beskid_analysis::syntax::SpanInfo,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Value, CodegenError> {
    match lower_lambda_function_value(lambda, span, ctx) {
        Ok(value) => Ok(value),
        Err(CodegenError::InvalidLocalBinding { .. }) => {
            Err(CodegenError::UnsupportedFeature("capturing lambda escape requires closure environment fat pointer"))
        }
        Err(err) => Err(err),
    }
}

pub(super) fn lower_function_typed_argument(
    arg_expr: &Spanned<HirExpressionNode>,
    expected_type: TypeId,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    if !matches!(ctx.type_result.types.get(expected_type), Some(TypeInfo::Function { .. })) {
        return Ok(None);
    }

    match &arg_expr.node {
        HirExpressionNode::LambdaExpression(lambda) => {
            Ok(Some(lower_lambda_function_value_checked(lambda, arg_expr.span, ctx)?))
        }
        HirExpressionNode::GroupedExpression(grouped) => {
            lower_function_typed_argument(&grouped.node.expr, expected_type, ctx)
        }
        HirExpressionNode::PathExpression(path_expr) => {
            match resolved_value_at(ctx.resolution, path_expr.node.path.span, ctx.codegen.current_source_path.as_ref())
            {
                Some(ResolvedValue::Local(local_id)) => {
                    if let Some(lambda_ptr) = ctx.state.local_lambdas.get(&local_id).copied() {
                        // SAFETY: pointer originates from immutable HIR nodes owned by lowering context.
                        let lambda = unsafe { lambda_ptr.as_ref() }.ok_or(CodegenError::UnsupportedNode {
                            span: arg_expr.span,
                            node: "dangling lambda binding",
                        })?;
                        Ok(Some(lower_lambda_function_value_checked(lambda, arg_expr.span, ctx)?))
                    } else {
                        Ok(None)
                    }
                }
                _ => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

pub(super) fn lower_local_lambda_call(
    node: &Spanned<HirCallExpression>,
    lambda: &Spanned<HirLambdaExpression>,
    ctx: &mut NodeLoweringContext<'_, '_>,
) -> Result<Option<Value>, CodegenError> {
    if lambda.node.parameters.len() != node.node.args.len() {
        return Err(CodegenError::UnsupportedNode { span: node.span, node: "lambda call arity mismatch" });
    }

    let mut previous_bindings: Vec<(beskid_analysis::resolve::LocalId, Option<Variable>)> =
        Vec::with_capacity(lambda.node.parameters.len());
    let mut previous_lambda_bindings: Vec<(
        beskid_analysis::resolve::LocalId,
        Option<*const Spanned<HirLambdaExpression>>,
    )> = Vec::with_capacity(lambda.node.parameters.len());

    for (parameter, arg_expr) in lambda.node.parameters.iter().zip(node.node.args.iter()) {
        let local_id =
            local_id_for_span(ctx.resolution, parameter.node.name.span, ctx.codegen.current_source_path.as_ref())
                .ok_or(CodegenError::InvalidLocalBinding { span: parameter.node.name.span })?;

        let expected_type = ctx
            .type_result
            .local_types
            .get(&local_id)
            .copied()
            .ok_or(CodegenError::MissingLocalType { span: parameter.node.name.span })?;

        let expected_is_function = matches!(ctx.type_result.types.get(expected_type), Some(TypeInfo::Function { .. }));
        if expected_is_function {
            let lambda_binding = match &arg_expr.node {
                HirExpressionNode::PathExpression(path_expr) => {
                    match resolved_value_at(
                        ctx.resolution,
                        path_expr.node.path.span,
                        ctx.codegen.current_source_path.as_ref(),
                    ) {
                        Some(ResolvedValue::Local(arg_local_id)) => ctx.state.local_lambdas.get(&arg_local_id).copied(),
                        _ => None,
                    }
                }
                HirExpressionNode::LambdaExpression(arg_lambda) => Some(arg_lambda as *const Spanned<_>),
                HirExpressionNode::GroupedExpression(grouped) => match &grouped.node.expr.node {
                    HirExpressionNode::LambdaExpression(arg_lambda) => Some(arg_lambda as *const Spanned<_>),
                    HirExpressionNode::PathExpression(path_expr) => {
                        match resolved_value_at(
                            ctx.resolution,
                            path_expr.node.path.span,
                            ctx.codegen.current_source_path.as_ref(),
                        ) {
                            Some(ResolvedValue::Local(arg_local_id)) => {
                                ctx.state.local_lambdas.get(&arg_local_id).copied()
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            };

            if let Some(lambda_binding) = lambda_binding {
                let previous = ctx.state.local_lambdas.insert(local_id, lambda_binding);
                previous_lambda_bindings.push((local_id, previous));
                continue;
            }
        }

        let arg_value = lower_node(arg_expr, ctx)?
            .ok_or(CodegenError::UnsupportedNode { span: arg_expr.span, node: "unit-valued lambda argument" })?;
        let actual_type = ctx.require_expr_type_for_node(arg_expr)?;
        let arg_value = ensure_type_compatibility_or_expected(
            arg_expr.span,
            expected_type,
            actual_type,
            ctx.type_result,
            ctx.resolution,
            ctx.builder,
            arg_value,
        )?;

        let clif_ty = map_type_id_to_clif(ctx.type_result, expected_type)
            .ok_or(CodegenError::UnsupportedNode { span: parameter.node.name.span, node: "lambda parameter type" })?;

        let var = ctx.builder.declare_var(clif_ty);
        ctx.builder.def_var(var, arg_value);

        let previous = ctx.state.locals.insert(local_id, var);
        previous_bindings.push((local_id, previous));
    }

    let result = lower_node(&lambda.node.body, ctx);

    for (local_id, previous) in previous_bindings {
        if let Some(var) = previous {
            ctx.state.locals.insert(local_id, var);
        } else {
            ctx.state.locals.remove(&local_id);
        }
    }
    for (local_id, previous) in previous_lambda_bindings {
        if let Some(lambda_ptr) = previous {
            ctx.state.local_lambdas.insert(local_id, lambda_ptr);
        } else {
            ctx.state.local_lambdas.remove(&local_id);
        }
    }

    result
}
