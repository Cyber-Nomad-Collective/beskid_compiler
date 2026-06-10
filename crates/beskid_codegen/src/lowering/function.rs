use crate::errors::CodegenError;
use crate::lowering::context::{CodegenContext, CodegenResult, LoweredFunction};
use crate::lowering::expressions::export::{export_linker_name, read_export_metadata, validate_export_function};
use crate::lowering::locals::local_id_for_span;
use crate::lowering::lowerable::lower_node;
use crate::lowering::node_context::NodeLoweringContext;
use crate::lowering::types::{
    map_type_id_to_clif, method_receiver_type_id, resolve_type_path_item_id_for_codegen,
    type_id_for_type,
};
use beskid_analysis::hir::{
    HirFunctionDefinition, HirLambdaExpression, HirMethodDefinition, HirPrimitiveType,
    HirTestDefinition, HirType,
};
use beskid_analysis::paths::same_file_opt;
use beskid_analysis::resolve::{ItemId, LocalId, Resolution, canonical_item_id};
use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use cranelift_codegen::ir::{AbiParam, Block, Function, InstBuilder, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use std::collections::HashMap;

pub(crate) fn lower_function(
    def: &Spanned<HirFunctionDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
) -> CodegenResult<()> {
    let saved_source_path = ctx.current_source_path.clone();
    if ctx.current_source_path.is_none() {
        ctx.current_source_path = resolution
            .items
            .iter()
            .find(|info| info.span == def.span)
            .and_then(|info| info.source_path.clone());
    }
    let result = lower_function_with_name(
        def,
        resolution,
        type_result,
        function_defs,
        ctx,
        None,
        None,
        None,
    );
    ctx.current_source_path = saved_source_path;
    result
}

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
        ctx.current_source_path = resolution
            .items
            .get(item_id.0)
            .and_then(|info| info.source_path.clone());
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
    let signature_types = type_result
        .function_signatures
        .get(&item_id)
        .or_else(|| type_result.method_function_signatures.get(&item_id));

    let receiver_type_id =
        method_receiver_type_id(resolution, type_result, &def.node.receiver_type, item_id).ok_or(
            CodegenError::UnsupportedNode {
                span: def.node.receiver_type.span,
                node: "method receiver type",
            },
        )?;
    let receiver_clif_ty = map_type_id_to_clif(type_result, receiver_type_id).ok_or(
        CodegenError::UnsupportedNode {
            span: def.node.receiver_type.span,
            node: "method receiver type",
        },
    )?;

    let mut signature = Signature::new(CallConv::SystemV);
    signature.params.push(AbiParam::new(receiver_clif_ty));
    for (index, param) in def.node.parameters.iter().enumerate() {
        let type_id = signature_types
            .and_then(|sig| sig.params.get(index).copied())
            .or_else(|| {
                type_id_for_type(
                    resolution,
                    type_result,
                    ctx.current_source_path.as_ref(),
                    &param.node.ty,
                )
            })
            .ok_or(CodegenError::UnsupportedNode {
                span: param.span,
                node: "function parameter type",
            })?;
        let clif_ty =
            map_type_id_to_clif(type_result, type_id).ok_or(CodegenError::UnsupportedNode {
                span: param.span,
                node: "function parameter type",
            })?;
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

    let this_local_id = local_id_for_span(
        resolution,
        def.node.receiver_type.span,
        ctx.current_source_path.as_ref(),
    )
    .ok_or(CodegenError::InvalidLocalBinding {
        span: def.node.receiver_type.span,
    })?;
    let this_var = builder.declare_var(receiver_clif_ty);
    builder.def_var(this_var, param_values[0]);
    state.locals.insert(this_local_id, this_var);
    state.parameter_locals.push(this_local_id);
    state
        .local_type_overrides
        .insert(this_local_id, receiver_type_id);

    for (index, (param, value)) in def
        .node
        .parameters
        .iter()
        .zip(param_values.iter().skip(1))
        .enumerate()
    {
        let local_id = local_id_for_span(
            resolution,
            param.node.name.span,
            ctx.current_source_path.as_ref(),
        )
        .ok_or(CodegenError::InvalidLocalBinding {
            span: param.node.name.span,
        })?;
        let type_id = type_result
            .local_types
            .get(&local_id)
            .copied()
            .or_else(|| signature_types.and_then(|sig| sig.params.get(index).copied()))
            .or_else(|| {
                type_id_for_type(
                    resolution,
                    type_result,
                    ctx.current_source_path.as_ref(),
                    &param.node.ty,
                )
            })
            .ok_or(CodegenError::MissingLocalType {
                span: param.node.name.span,
            })?;
        let clif_ty =
            map_type_id_to_clif(type_result, type_id).ok_or(CodegenError::UnsupportedNode {
                span: param.node.name.span,
                node: "function parameter type",
            })?;
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
        receiver_type: Some(receiver_type_id),
    };

    for statement in &def.node.body.node.statements {
        lower_node(statement, &mut node_ctx)?;
        if node_ctx.state.block_terminated {
            break;
        }
    }

    if !node_ctx.state.return_emitted && !node_ctx.state.block_terminated {
        if expects_return {
            return Err(CodegenError::UnsupportedNode {
                span: def.span,
                node: "implicit non-unit return",
            });
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
        return Err(CodegenError::VerificationFailed {
            function: function_name.clone(),
            message: err.to_string(),
        });
    }

    ctx.functions_emitted += 1;
    ctx.lowered_functions.push(LoweredFunction {
        name: function_name,
        function,
    });
    Ok(())
}

pub(crate) fn lower_test(
    def: &Spanned<HirTestDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
) -> CodegenResult<()> {
    let item_id = resolution
        .items
        .iter()
        .find(|info| info.span == def.span)
        .map(|info| info.id);
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
        receiver_type: None,
    };

    for statement in &def.node.body.node.statements {
        lower_node(statement, &mut node_ctx)?;
        if node_ctx.state.block_terminated {
            break;
        }
    }

    if !node_ctx.state.return_emitted && !node_ctx.state.block_terminated {
        if expects_return {
            return Err(CodegenError::UnsupportedNode {
                span: def.span,
                node: "implicit non-unit return",
            });
        }
        node_ctx.builder.ins().return_(&[]);
    }

    drop(node_ctx);
    builder.finalize();

    let function_name = def.node.name.node.name.clone();
    let flags = settings::Flags::new(settings::builder());
    if let Err(err) = verify_function(&function, &flags) {
        return Err(CodegenError::VerificationFailed {
            function: function_name.clone(),
            message: err.to_string(),
        });
    }

    ctx.functions_emitted += 1;
    ctx.lowered_functions.push(LoweredFunction {
        name: function_name,
        function,
    });
    Ok(())
}

pub(crate) fn mangle_method_name(receiver: &str, method: &str) -> String {
    let receiver_short = receiver.rsplit("::").next().unwrap_or(receiver);
    let method_short = method.rsplit("::").next().unwrap_or(method);
    format!("__method__{receiver_short}__{method_short}")
}

pub(crate) fn is_self_parameter_function(def: &HirFunctionDefinition) -> bool {
    def.parameters
        .first()
        .is_some_and(|param| param.node.name.node.name == "self")
}

pub(crate) fn generic_mapping_for_method_receiver(
    type_result: &TypeResult,
    item_id: ItemId,
    receiver_type: TypeId,
) -> HashMap<String, TypeId> {
    let mut mapping = HashMap::new();
    let Some(method_generic_names) = type_result.generic_items.get(&item_id) else {
        return mapping;
    };
    let Some(TypeInfo::Applied { base, args }) = type_result.types.get(receiver_type) else {
        return mapping;
    };
    if method_generic_names.len() == 1 && args.len() == 1 {
        mapping.insert(method_generic_names[0].clone(), args[0]);
        return mapping;
    }
    if let Some(type_generic_names) = type_result.generic_items.get(base) {
        for (name, arg) in type_generic_names.iter().zip(args.iter()) {
            mapping.insert(name.clone(), *arg);
        }
    }
    mapping
}

pub(crate) fn mangle_function_name(base: &str, args: &[beskid_analysis::types::TypeId]) -> String {
    if args.is_empty() {
        return base.to_string();
    }
    let suffix = args
        .iter()
        .map(|arg| arg.0.to_string())
        .collect::<Vec<_>>()
        .join("_");
    format!("{base}#{suffix}")
}

/// Disambiguate non-generic link-plan functions that share a short name across modules (`Contains#42`).
pub(crate) fn mangle_item_function(resolution: &Resolution, item_id: ItemId) -> String {
    let info = resolution
        .items
        .get(item_id.0)
        .unwrap_or_else(|| panic!("missing item for mangling: {:?}", item_id));
    let short = info.name.rsplit("::").next().unwrap_or(info.name.as_str());
    format!("{short}#{}", item_id.0)
}

pub(crate) fn linker_name_for_item_function(
    resolution: &Resolution,
    item_id: ItemId,
    def: &Spanned<HirFunctionDefinition>,
) -> String {
    if read_export_metadata(def).is_some() {
        export_linker_name(def)
    } else {
        mangle_item_function(resolution, item_id)
    }
}

/// Stem-qualified mangling for generic factory functions on owning types (`Hub__Create#2`).
pub(crate) fn mangle_generic_factory_name(
    owner_stem: &str,
    method: &str,
    args: &[TypeId],
) -> String {
    let leaf = method.rsplit("::").next().unwrap_or(method);
    mangle_function_name(&format!("{owner_stem}__{leaf}"), args)
}

/// When a generic function returns `Owner<T>` from the same source file as `Owner`, qualify the symbol stem.
pub(crate) fn owner_stem_for_generic_factory(
    item_id: ItemId,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<String> {
    let generic_names = type_result.generic_items.get(&item_id)?;
    if generic_names.is_empty() {
        return None;
    }
    let sig = type_result.function_signatures.get(&item_id)?;
    let TypeInfo::Applied { base, .. } = type_result.types.get(sig.return_type)? else {
        return None;
    };
    let func_info = resolution.items.get(item_id.0)?;
    let owner_info = resolution.items.iter().find(|info| info.id == *base)?;
    if !same_file_opt(
        func_info.source_path.as_ref(),
        owner_info.source_path.as_ref(),
    ) {
        return None;
    }
    owner_info
        .name
        .rsplit("::")
        .next()
        .map(str::to_string)
}

pub(crate) fn mangle_generic_item_function(
    item_id: ItemId,
    base: &str,
    generic_args: &[TypeId],
    resolution: &Resolution,
    type_result: &TypeResult,
) -> String {
    let leaf = base.rsplit("::").next().unwrap_or(base);
    if !generic_args.is_empty()
        && let Some(stem) = owner_stem_for_generic_factory(item_id, resolution, type_result)
    {
        return mangle_generic_factory_name(&stem, leaf, generic_args);
    }
    mangle_function_name(leaf, generic_args)
}

/// Recover generic substitution from a monomorph symbol name (`Equal#5` or `Hub__Create#2`).
pub(crate) fn generic_mapping_from_mangled(
    type_result: &TypeResult,
    item_id: ItemId,
    mangled: &str,
) -> Option<HashMap<String, TypeId>> {
    let generic_names = type_result.generic_items.get(&item_id)?;
    let suffix = mangled.rsplit('#').next()?;
    if suffix == mangled {
        return None;
    }
    let type_ids: Vec<TypeId> = suffix
        .split('_')
        .filter_map(|part| part.parse::<usize>().ok())
        .map(TypeId)
        .collect();
    if type_ids.len() != generic_names.len() {
        return None;
    }
    Some(
        generic_names
            .iter()
            .cloned()
            .zip(type_ids)
            .collect(),
    )
}

fn substitute_type_id(
    type_result: &TypeResult,
    type_id: beskid_analysis::types::TypeId,
    mapping: &HashMap<String, beskid_analysis::types::TypeId>,
) -> beskid_analysis::types::TypeId {
    let info = type_result.types.get(type_id).cloned();
    match info {
        Some(TypeInfo::GenericParam(name)) => mapping.get(&name).copied().unwrap_or(type_id),
        Some(TypeInfo::Applied { .. }) => type_id,
        _ => type_id,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_function_with_name(
    def: &Spanned<HirFunctionDefinition>,
    resolution: &Resolution,
    type_result: &TypeResult,
    function_defs: &HashMap<ItemId, &Spanned<HirFunctionDefinition>>,
    ctx: &mut CodegenContext,
    name_override: Option<String>,
    generic_args: Option<HashMap<String, beskid_analysis::types::TypeId>>,
    known_item_id: Option<ItemId>,
) -> CodegenResult<()> {
    let generic_args = generic_args.unwrap_or_default();
    let item_id = known_item_id
        .or_else(|| item_id_for_item_span(resolution, def.span, ctx.current_source_path.as_ref()))
        .map(|id| canonical_item_id(resolution, id));
    if let Some(id) = item_id {
        ctx.emitting_items.insert(id);
    }
    let saved_substitution = std::mem::take(&mut ctx.active_generic_substitution);
    if !generic_args.is_empty() {
        ctx.active_generic_substitution = generic_args.clone();
    }
    let result = lower_function_with_name_body(
        def,
        resolution,
        type_result,
        function_defs,
        ctx,
        name_override,
        &generic_args,
        item_id,
    );
    ctx.active_generic_substitution = saved_substitution;
    finish_emitting(ctx, item_id);
    result
}

fn finish_emitting(ctx: &mut CodegenContext, item_id: Option<ItemId>) {
    if let Some(id) = item_id {
        ctx.emitting_items.remove(&id);
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_function_with_name_body(
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
            .or_else(|| {
                type_id_for_type(
                    resolution,
                    type_result,
                    ctx.current_source_path.as_ref(),
                    &param.node.ty,
                )
            })
            .map(&substitute)
            .ok_or(CodegenError::UnsupportedNode {
                span: param.span,
                node: "function parameter type",
            })?;
        let clif_ty =
            map_type_id_to_clif(type_result, type_id).ok_or(CodegenError::UnsupportedNode {
                span: param.span,
                node: "function parameter type",
            })?;
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
    for (index, (param, value)) in def
        .node
        .parameters
        .iter()
        .zip(param_values)
        .enumerate()
    {
        let local_id = local_id_for_span(
            resolution,
            param.node.name.span,
            ctx.current_source_path.as_ref(),
        )
        .ok_or(CodegenError::InvalidLocalBinding {
            span: param.node.name.span,
        })?;
        let type_id = signature_types
            .and_then(|sig| sig.params.get(index).copied())
            .or_else(|| type_result.local_types.get(&local_id).copied())
            .or_else(|| {
                type_id_for_type(
                    resolution,
                    type_result,
                    ctx.current_source_path.as_ref(),
                    &param.node.ty,
                )
            })
            .map(&substitute)
            .ok_or(CodegenError::MissingLocalType {
                span: param.node.name.span,
            })?;
        let clif_ty =
            map_type_id_to_clif(type_result, type_id).ok_or(CodegenError::UnsupportedNode {
                span: param.node.name.span,
                node: "function parameter type",
            })?;
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
        receiver_type: None,
    };

    for statement in &def.node.body.node.statements {
        lower_node(statement, &mut node_ctx)?;
        if node_ctx.state.block_terminated {
            break;
        }
    }

    if !node_ctx.state.return_emitted && !node_ctx.state.block_terminated {
        if expects_return {
            return Err(CodegenError::UnsupportedNode {
                span: def.span,
                node: "implicit non-unit return",
            });
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
        item_id
            .map(|id| linker_name_for_item_function(resolution, id, def))
            .unwrap_or_else(|| export_linker_name(def))
    });
    ctx.lowered_functions.push(LoweredFunction {
        name: function_name,
        function,
    });

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
pub(crate) fn refresh_locals_at_loop_header(
    builder: &mut FunctionBuilder,
    state: &FunctionLoweringState,
) {
    for local_id in &state.parameter_locals {
        let Some(var) = state.locals.get(local_id) else {
            continue;
        };
        let value = builder.use_var(*var);
        builder.def_var(*var, value);
    }
}

fn signature_has_return(signature: &Signature) -> bool {
    !signature.returns.is_empty()
}

fn resolve_return_type_id(
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&std::path::PathBuf>,
    return_type: Option<&Spanned<HirType>>,
    signature_return: Option<TypeId>,
) -> Option<TypeId> {
    if let Some(annotated) = return_type
        .and_then(|ty| type_id_for_type(resolution, type_result, source_path, ty))
    {
        return Some(annotated);
    }
    return_type
        .and_then(|ty| {
            fallback_applied_return_type(resolution, type_result, source_path, ty)
        })
        .or_else(|| {
            signature_return.filter(|sig| {
                !matches!(
                    type_result.types.get(*sig),
                    Some(TypeInfo::Primitive(HirPrimitiveType::Unit))
                )
            })
        })
}

fn fallback_applied_return_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&std::path::PathBuf>,
    return_type: &Spanned<HirType>,
) -> Option<TypeId> {
    let beskid_analysis::hir::HirType::Complex(path) = &return_type.node else {
        return None;
    };
    let last = path.node.segments.last()?;
    if last.node.type_args.is_empty() {
        return None;
    }
    let segments: Vec<String> = path
        .node
        .segments
        .iter()
        .map(|segment| segment.node.name.node.name.clone())
        .collect();
    let mut arg_ids = Vec::with_capacity(last.node.type_args.len());
    for arg in &last.node.type_args {
        let type_id = type_id_for_type(resolution, type_result, source_path, arg).or_else(|| {
            match &arg.node {
                HirType::Primitive(primitive) => {
                    find_primitive_type_id(type_result, primitive.node)
                }
                HirType::Complex(path) => {
                    let name = path.node.segments.last()?.node.name.node.name.as_str();
                    match name {
                        "i64" => find_primitive_type_id(type_result, HirPrimitiveType::I64),
                        "string" => find_primitive_type_id(type_result, HirPrimitiveType::String),
                        _ => find_named_type_by_leaf(type_result, name),
                    }
                }
                _ => None,
            }
        })?;
        arg_ids.push(type_id);
    }
    if let Some(base) = resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
        && let Some(applied) = find_applied_type_id_by_base_and_args(type_result, base, &arg_ids) {
            return Some(applied);
        }
    find_applied_type_id_by_args(type_result, &arg_ids).or_else(|| {
        find_applied_type_id_by_ok_arg(type_result, arg_ids.first().copied())
    })
}

fn find_applied_type_id_by_base_and_args(
    type_result: &TypeResult,
    base: ItemId,
    args: &[TypeId],
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied {
            base: found_base,
            args: found_args,
        } = info
            && *found_base == base
            && found_args.as_slice() == args
        {
            return Some(type_id);
        }
        index += 1;
    }
}

fn find_applied_type_id_by_ok_arg(
    type_result: &TypeResult,
    ok_type: Option<TypeId>,
) -> Option<TypeId> {
    let ok_type = ok_type?;
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied { base, args } = info
            && args.first() == Some(&ok_type)
            && args.len() == 2
            && type_result.enum_variants_ordered.contains_key(base)
        {
            return Some(type_id);
        }
        index += 1;
    }
}

fn find_primitive_type_id(type_result: &TypeResult, primitive: HirPrimitiveType) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Primitive(found) if *found == primitive) {
            return Some(type_id);
        }
        index += 1;
    }
}

fn find_named_type_by_leaf(type_result: &TypeResult, leaf: &str) -> Option<TypeId> {
    for (item_id, name) in &type_result.named_type_names {
        if name.as_str() == leaf || name.ends_with(&format!("::{leaf}")) {
            let mut index = 0usize;
            loop {
                let type_id = TypeId(index);
                let Some(info) = type_result.types.get(type_id) else {
                    break;
                };
                if matches!(info, TypeInfo::Named(found) if *found == *item_id) {
                    return Some(type_id);
                }
                index += 1;
            }
        }
    }
    None
}

fn find_applied_type_id_by_args(type_result: &TypeResult, args: &[TypeId]) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied {
            args: found_args, ..
        } = info
            && found_args.as_slice() == args
        {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn item_id_for_item_span(
    resolution: &Resolution,
    span: SpanInfo,
    source_path: Option<&std::path::PathBuf>,
) -> Option<ItemId> {
    if let Some(path) = source_path
        && let Some(info) = resolution
            .items
            .iter()
            .find(|info| info.span == span && same_file_opt(info.source_path.as_ref(), Some(path)))
        {
            return Some(info.id);
        }

    let matches: Vec<_> = resolution
        .items
        .iter()
        .filter(|info| info.span == span)
        .collect();
    match matches.as_slice() {
        [] => None,
        [single] => Some(single.id),
        _ => None,
    }
}
