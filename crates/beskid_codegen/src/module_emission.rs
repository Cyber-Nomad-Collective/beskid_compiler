use std::collections::{HashMap, HashSet};
use std::time::Instant;

use beskid_analysis::types::TypeId;
use beskid_isle::{AstNodeKey, DirectCallee, FunctionEmissionError, StringInterner};
use beskid_queries::{
    CallLowering, GenericSpecializationInstance, GenericSubstitution, ItemSignature, SemanticTypeId, SourceUnitId,
    call_lowering, child_nodes, closure_call_target, closure_environment, closure_signature,
    extern_contract_import_for_declaration, format_ast_node_key, format_ast_node_site, generic_call_specialization, generic_call_template,
    generic_specialization_identity, generic_specialization_instance, item_abi_signature, item_name, node_kind,
    node_span, resolved_item, spawn_entry_validation,
};
use cranelift_codegen::ir::{AbiParam, InstBuilder};
use cranelift_codegen::ir::{
    Endianness, ExtFuncData, ExternalName, FuncRef, Function, GlobalValueData, Signature, Type, Value, types,
};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, ModuleError, ModuleResult};

use crate::aggregate_static::{ABI_V5_MANAGED_OBJECT_ALLOCATE, AggregateStaticPlan, emit_aggregate_static_data};
use crate::array_static::{
    ABI_V5_ARRAY_ALLOCATE_ROOTED, ABI_V5_ARRAY_CONSTRUCTION_FINISH, ArrayStaticPlan, emit_array_static_data,
};
use crate::closure_static::{
    ABI_V5_CLOSURE_CAPTURE_STORE, ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE, ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT,
    ClosureStaticPlan, emit_closure_static_data,
};
use crate::lowering::descriptor::TypeDescriptorData;
use crate::lowering::{CodegenArtifact, ExternImport};
use crate::{
    CodegenContext, CodegenInput, emit_isle_closure_lambda_entry, emit_isle_expression_with_call_importer,
    emit_isle_item_with_services, emit_isle_item_with_services_specialization,
};

const ABI_V5_FIBER_SPAWN_WITH_CANCEL_SLOT: &str = "beskid_rt_v5_fiber_spawn_with_cancel_slot";

/// Cranelift [`DataId`] pair for a type: main descriptor blob and companion pointer-offset table.
#[derive(Debug, Clone)]
pub struct DescriptorHandles {
    pub descriptor: DataId,
    pub offsets: DataId,
}

/// One syntax item declared and defined through the HIR-free ISLE boundary.
#[derive(Debug, Clone)]
pub struct SyntaxModuleItem {
    pub key: AstNodeKey,
    pub symbol: String,
}

/// State owned by a long-lived Cranelift module while it receives source artifacts.
///
/// A session supplies one namespace for source-owned metadata and remembers function handles by
/// final symbol. Re-emitting the same source artifact therefore returns its existing handles
/// rather than redeclaring a conflicting module symbol. Callers that emit independent artifacts
/// choose distinct namespaces; the convenience API below keeps the historical one-shot behavior.
#[derive(Debug, Clone)]
pub struct ModuleEmissionSession {
    namespace: std::sync::Arc<str>,
    callees: HashMap<DirectCallee, FuncId>,
    source_artifacts: HashSet<String>,
}

impl ModuleEmissionSession {
    pub fn new(namespace: impl Into<std::sync::Arc<str>>) -> Self {
        Self { namespace: namespace.into(), callees: HashMap::new(), source_artifacts: HashSet::new() }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }
}

/// One fully declared syntax item after generic source declarations have been expanded into
/// exact ABI specializations.  The callee key is the same structural identity produced by ISLE
/// call facts, keeping declaration and import selection generation-safe.
#[derive(Debug, Clone)]
struct ResolvedSyntaxModuleItem {
    key: AstNodeKey,
    symbol: String,
    callee: DirectCallee,
    specialization: Option<GenericSpecializationInstance>,
}

#[derive(Debug, Clone)]
struct SpawnTrampoline {
    spawn: AstNodeKey,
    target_symbol: String,
    target_signature: Signature,
    lambda_body: Option<AstNodeKey>,
    /// Present when the trampoline target is a capturing lambda that reads from the environment.
    closure_captures: Option<Vec<beskid_isle::InlineCaptureField>>,
    symbol: String,
}

/// One freestanding lambda lowered to its own trampoline function.
#[derive(Debug, Clone)]
struct LambdaTrampoline {
    lambda: AstNodeKey,
    lambda_body: AstNodeKey,
    target_signature: Signature,
    closure_captures: Option<Vec<beskid_isle::InlineCaptureField>>,
    symbol: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SyntaxModuleEmissionError {
    #[error("module declaration failed: {0}")]
    Module(#[from] ModuleError),
    /// Pre-formatted with [`FunctionEmissionError::display_with_db`] so FAIL lines include
    /// construct and source range, not only `#gN:nN`.
    #[error("syntax ISLE emission failed: {0}")]
    Emission(String),
    #[error("syntax module declares duplicate symbol `{0}`")]
    DuplicateSymbol(String),
}

fn emission_error(input: &CodegenInput<'_>, error: FunctionEmissionError) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(error.display_with_db(input.database()))
}

fn emission_verification(message: impl Into<String>) -> SyntaxModuleEmissionError {
    SyntaxModuleEmissionError::Emission(format!("Verification({})", message.into()))
}

/// Lower syntax items into the ordinary backend artifact boundary without constructing HIR or
/// using the legacy `Lowerable` implementation. Direct calls retain their exact syntax item
/// identity while their emitted CLIF references the final declared symbol by name.
///
/// Backends may then use their existing artifact declaration/remapping path. Production JIT
/// entrypoint lowering uses this bridge as it migrates away from HIR.
pub fn lower_syntax_program(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[SyntaxModuleItem],
) -> Result<CodegenArtifact, SyntaxModuleEmissionError> {
    let started = Instant::now();
    crate::isle_trace::event(|| format!("event=clif.begin items={} roots={}", items.len(), input.roots().len()));
    let items = match resolve_module_items(input, items).and_then(|items| expand_direct_spawn_items(input, items)) {
        Ok(items) => items,
        Err(error) => {
            crate::isle_trace::event(|| format!("event=isle.missing rule=module_item_resolution detail={error}"));
            return Err(error);
        }
    };
    let result = lower_resolved_syntax_program(input, isa, &items);
    crate::isle_trace::event(|| match &result {
        Ok(artifact) => format!(
            "event=clif.end outcome=ok elapsed_ms={} functions={} imports={}",
            started.elapsed().as_millis(),
            artifact.functions.len(),
            artifact.extern_imports.len()
        ),
        Err(error) => {
            format!("event=clif.end outcome=error elapsed_ms={} detail={error}", started.elapsed().as_millis())
        }
    });
    result
}

/// Spawn has no ordinary CallExpression edge, so the generic direct-call reachability query does
/// not include its target. Add only entries proven by the same strict direct-item validation used
/// for trampoline generation; this does not make lambda or argument-bearing spawns reachable.
fn expand_direct_spawn_items(
    input: &CodegenInput<'_>,
    mut items: Vec<ResolvedSyntaxModuleItem>,
) -> Result<Vec<ResolvedSyntaxModuleItem>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut cursor = 0;
    while cursor < items.len() {
        let mut spawns = Vec::new();
        collect_spawn_nodes(db, items[cursor].key, &mut HashSet::new(), &mut spawns);
        for spawn in spawns {
            let Some(validation) =
                spawn_entry_validation(db, spawn).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            if !validation.is_zero_argument_entry
                || node_kind(db, validation.target).map_err(|error| emission_verification(error.to_string()))?
                    != Some(beskid_queries::IndexedNodeKind::PathExpression)
            {
                continue;
            }
            let Some(target) =
                resolved_item(db, validation.target).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            if items.iter().any(|item| item.key == target.declaration) {
                continue;
            }
            let Some(symbol) = syntax_item_symbol(input, target.declaration) else {
                continue;
            };
            if item_abi_signature(db, target.declaration)
                .map_err(|error| emission_verification(error.to_string()))?
                .is_none()
            {
                continue;
            }
            items.push(ResolvedSyntaxModuleItem {
                key: target.declaration,
                symbol,
                callee: DirectCallee::item(target.declaration),
                specialization: None,
            });
        }
        cursor += 1;
    }
    Ok(items)
}

fn syntax_item_symbol(input: &CodegenInput<'_>, key: AstNodeKey) -> Option<String> {
    let name = item_name(input.database(), key).ok().flatten()?;
    let unit = input
        .typed_program()
        .assembly
        .units()
        .iter()
        .find(|unit| SourceUnitId::new(input.database(), unit.path.clone()) == key.unit)?;
    let logical = unit
        .logical_name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect::<String>();
    Some(format!("{name}#syntax_{logical}_{}", key.node.0))
}

fn lower_resolved_syntax_program(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
) -> Result<CodegenArtifact, SyntaxModuleEmissionError> {
    let mut symbols = HashMap::with_capacity(items.len());
    for item in items {
        if symbols.insert(item.callee.clone(), item.symbol.clone()).is_some() {
            return Err(SyntaxModuleEmissionError::DuplicateSymbol(item.symbol.clone()));
        }
    }
    let runtime_intrinsics = runtime_intrinsic_symbols(input);
    symbols.extend(runtime_intrinsics.iter().map(|(callee, symbol)| (callee.clone(), symbol.clone())));
    let corelib_services = corelib_service_symbols(input, items);
    symbols.extend(corelib_services.iter().map(|(callee, symbol)| (callee.clone(), symbol.clone())));
    let extern_contracts = extern_contract_symbols(input, items);
    symbols.extend(extern_contracts.iter().map(|(callee, symbol)| (callee.clone(), symbol.clone())));

    let trampolines = resolve_spawn_trampolines(input, isa, items, &symbols)?;
    symbols.extend(
        trampolines
            .iter()
            .map(|trampoline| (DirectCallee::spawn_trampoline(trampoline.spawn), trampoline.symbol.clone())),
    );

    let lambda_trampolines = resolve_lambda_trampolines(input, isa, items, &symbols)?;
    symbols.extend(
        lambda_trampolines
            .iter()
            .map(|trampoline| (DirectCallee::lambda_trampoline(trampoline.lambda), trampoline.symbol.clone())),
    );

    let mut context = CodegenContext::new_with_artifact_namespace(input.artifact_namespace().to_owned());
    let lambda_count =
        trampolines.iter().filter(|trampoline| trampoline.lambda_body.is_some()).count() + lambda_trampolines.len();
    let mut functions = Vec::with_capacity(items.len() + trampolines.len() + lambda_count + lambda_trampolines.len());
    for trampoline in &trampolines {
        if let Some(body) = trampoline.lambda_body {
            let result = trampoline
                .target_signature
                .returns
                .first()
                .map(|parameter| parameter.value_type)
                .ok_or_else(|| emission_verification("spawned lambda entry must return an ABI value"))?;
            let function = {
                let mut importer = ArtifactCallImporter { symbols: &symbols };
                if let Some(captures) = &trampoline.closure_captures {
                    emit_isle_closure_lambda_entry(input, isa, body, result, captures, &mut importer)
                        .map_err(|error| emission_error(input, error))?
                } else {
                    emit_isle_expression_with_call_importer(input, isa, body, result, &mut importer)
                        .map_err(|error| emission_error(input, error))?
                }
            };
            functions.push(crate::LoweredFunction { name: trampoline.target_symbol.clone(), function });
        }
        functions.push(crate::LoweredFunction {
            name: trampoline.symbol.clone(),
            function: emit_spawn_trampoline(trampoline, isa)?,
        });
    }
    for trampoline in &lambda_trampolines {
        let result = trampoline
            .target_signature
            .returns
            .first()
            .map(|parameter| parameter.value_type)
            .ok_or_else(|| emission_verification("lambda entry must return an ABI value"))?;
        let function = {
            let mut importer = ArtifactCallImporter { symbols: &symbols };
            if let Some(captures) = &trampoline.closure_captures {
                emit_isle_closure_lambda_entry(input, isa, trampoline.lambda_body, result, captures, &mut importer)
                    .map_err(|error| emission_error(input, error))?
            } else {
                emit_isle_expression_with_call_importer(input, isa, trampoline.lambda_body, result, &mut importer)
                    .map_err(|error| emission_error(input, error))?
            }
        };
        functions.push(crate::LoweredFunction { name: trampoline.symbol.clone(), function });
    }
    for item in items {
        trace_item_facts(input, item.key, &symbols);
        let started = Instant::now();
        crate::isle_trace::event(|| {
            format!(
                "event=isle.selected rule=emit_item_statement item={} symbol={}",
                trace_key(input.database(), item.key),
                item.symbol
            )
        });
        let function = {
            let mut importer = ArtifactCallImporter { symbols: &symbols };
            let mut strings = ArtifactStringInterner { context: &mut context, pointer_type: isa.pointer_type() };
            match &item.specialization {
                Some(specialization) => emit_isle_item_with_services_specialization(
                    input,
                    isa,
                    item.key,
                    specialization.clone(),
                    &mut strings,
                    &mut importer,
                ),
                None => emit_isle_item_with_services(input, isa, item.key, &mut strings, &mut importer),
            }
            .map_err(|error| {
                crate::isle_trace::event(|| {
                    format!(
                        "event=isle.missing rule=emit_item_statement item={} elapsed_ms={} detail={}",
                        beskid_queries::format_ast_node_site(input.database(), item.key),
                        started.elapsed().as_millis(),
                        error.display_with_db(input.database()),
                    )
                });
                emission_error(input, error)
            })?
        };
        crate::isle_trace::event(|| {
            format!(
                "event=isle.emitted item={} elapsed_ms={}",
                trace_key(input.database(), item.key),
                started.elapsed().as_millis(),
            )
        });
        functions.push(crate::LoweredFunction { name: item.symbol.clone(), function });
    }
    let mut extern_imports = runtime_intrinsics
        .into_values()
        .chain(corelib_services.into_values())
        .chain((!trampolines.is_empty()).then_some(ABI_V5_FIBER_SPAWN_WITH_CANCEL_SLOT.to_owned()))
        .map(|symbol| ExternImport { symbol, abi: Some("C".into()), library: None })
        .collect::<Vec<_>>();
    for import in extern_contract_imports(input, items) {
        if !extern_imports.iter().any(|existing| existing.symbol == import.symbol) {
            extern_imports.push(import);
        }
    }

    let closure_static_plans = collect_closure_static_plans(input, items, &trampolines, &lambda_trampolines);
    if !closure_static_plans.is_empty() {
        for symbol in
            [ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE, ABI_V5_CLOSURE_CAPTURE_STORE, ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT]
        {
            if !extern_imports.iter().any(|existing| existing.symbol == symbol) {
                extern_imports.push(ExternImport { symbol: symbol.to_owned(), abi: Some("C".into()), library: None });
            }
        }
    }
    let aggregate_static_plans = collect_aggregate_static_plans(input, items);
    if !aggregate_static_plans.is_empty()
        && !extern_imports.iter().any(|existing| existing.symbol == ABI_V5_MANAGED_OBJECT_ALLOCATE)
    {
        extern_imports.push(ExternImport {
            symbol: ABI_V5_MANAGED_OBJECT_ALLOCATE.to_owned(),
            abi: Some("C".into()),
            library: None,
        });
    }
    let array_static_plans = collect_array_static_plans(input, items);
    if !array_static_plans.is_empty() {
        for symbol in
            [ABI_V5_ARRAY_ALLOCATE_ROOTED, ABI_V5_ARRAY_CONSTRUCTION_FINISH, "beskid_rt_v5_array_write_barrier"]
        {
            if !extern_imports.iter().any(|existing| existing.symbol == symbol) {
                extern_imports.push(ExternImport { symbol: symbol.to_owned(), abi: Some("C".into()), library: None });
            }
        }
    }

    Ok(CodegenArtifact {
        functions,
        string_literals: context.string_literals,
        extern_imports,
        closure_static_plans,
        aggregate_static_plans,
        array_static_plans,
        ..CodegenArtifact::default()
    })
}

fn collect_array_static_plans(input: &CodegenInput<'_>, items: &[ResolvedSyntaxModuleItem]) -> Vec<ArrayStaticPlan> {
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
    }
    nodes.into_iter().filter_map(|key| input.array_static_plan(key)).collect()
}

fn collect_aggregate_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Vec<AggregateStaticPlan> {
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
    }
    nodes
        .into_iter()
        .filter_map(|key| input.aggregate_static_plan(key).or_else(|| input.enum_static_plan(key)))
        .collect()
}

/// Resolve source-proven zero-argument entries without ever re-entering HIR lowering.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
fn collect_closure_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    trampolines: &[SpawnTrampoline],
    lambda_trampolines: &[LambdaTrampoline],
) -> Vec<ClosureStaticPlan> {
    let db = input.database();
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    let mut push_plan = |plan: ClosureStaticPlan| {
        if seen.insert(plan.lambda) {
            plans.push(plan);
        }
    };
    for trampoline in trampolines {
        if trampoline.closure_captures.is_some()
            && let Ok(Some(validation)) = spawn_entry_validation(db, trampoline.spawn)
            && let Some(authority) = input.closure_lowering_authority(trampoline.spawn, validation.target)
        {
            push_plan(authority.plan);
        }
    }
    for trampoline in lambda_trampolines {
        if trampoline.closure_captures.is_some()
            && let Some(authority) = input.closure_lowering_authority(trampoline.lambda, trampoline.lambda)
        {
            push_plan(authority.plan);
        }
    }
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(db, item.key, &mut visited, &mut nodes);
    }
    for key in nodes {
        if let Ok(Some(target)) = closure_call_target(db, key)
            && let Some(authority) = input.closure_lowering_authority(key, target.lambda)
        {
            push_plan(authority.plan);
        }
    }
    plans
}

fn collect_ast_nodes(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    nodes: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    nodes.push(key);
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_ast_nodes(db, child, visited, nodes);
        }
    }
}

/// Resolve source-proven zero-argument entries without ever re-entering HIR lowering.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
fn resolve_spawn_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
) -> Result<Vec<SpawnTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut spawns = Vec::new();
    let mut visited = HashSet::new();
    for item in items {
        collect_spawn_nodes(db, item.key, &mut visited, &mut spawns);
    }
    let mut trampolines = Vec::new();
    for spawn in spawns {
        let Some(validation) =
            spawn_entry_validation(db, spawn).map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        if !validation.is_zero_argument_entry {
            continue;
        }
        match node_kind(db, validation.target).map_err(|error| emission_verification(error.to_string()))? {
            Some(beskid_queries::IndexedNodeKind::PathExpression) => {
                let Some(target) =
                    resolved_item(db, validation.target).map_err(|error| emission_verification(error.to_string()))?
                else {
                    continue;
                };
                let Some(signature) = item_abi_signature(db, target.declaration)
                    .map_err(|error| emission_verification(error.to_string()))?
                    .and_then(|signature| spawn_target_signature(isa, signature))
                else {
                    continue;
                };
                if !signature.params.is_empty() {
                    continue;
                }
                let callee = DirectCallee::item(target.declaration);
                let Some(target_symbol) = symbols.get(&callee).cloned() else {
                    continue;
                };
                let symbol = spawn_trampoline_symbol(&target_symbol, spawn);
                trampolines.push(SpawnTrampoline {
                    spawn,
                    target_symbol,
                    target_signature: signature,
                    lambda_body: None,
                    closure_captures: None,
                    symbol,
                });
            }
            Some(beskid_queries::IndexedNodeKind::LambdaExpression) => {
                let Some(environment) = closure_environment(db, validation.target)
                    .map_err(|error| emission_verification(error.to_string()))?
                else {
                    continue;
                };
                let closure_captures = if environment.captures.is_empty() {
                    None
                } else {
                    let Some(authority) = input.closure_lowering_authority(spawn, validation.target) else {
                        continue;
                    };
                    let Some(captures) = authority
                        .plan
                        .captures
                        .iter()
                        .map(|field| {
                            Some(beskid_isle::InlineCaptureField {
                                local_slot: beskid_isle::LocalSlotId {
                                    owner_node: field.capture.slot.owner.node.0,
                                    index: field.capture.slot.index,
                                },
                                field_offset: u32::try_from(field.field_offset).ok()?,
                                pointer_map_index: field.pointer_map_index,
                                value_type: map_spawn_capture_type(isa, field.abi_type)?,
                            })
                        })
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    Some(captures)
                };
                let Some(lambda) = closure_signature(db, validation.target)
                    .map_err(|error| emission_verification(error.to_string()))?
                else {
                    continue;
                };
                let Some(mut signature) = spawn_target_signature(isa, lambda.callable) else {
                    continue;
                };
                if !signature.params.is_empty() {
                    continue;
                }
                if closure_captures.is_some() {
                    signature.params.insert(0, AbiParam::new(isa.pointer_type()));
                }
                let target_symbol = format!("__beskid_spawn_lambda_syntax_g{}_n{}", spawn.generation.0, spawn.node.0);
                let symbol = spawn_trampoline_symbol(&target_symbol, spawn);
                trampolines.push(SpawnTrampoline {
                    spawn,
                    target_symbol,
                    target_signature: signature,
                    lambda_body: Some(lambda.body),
                    closure_captures,
                    symbol,
                });
            }
            _ => continue,
        }
    }
    Ok(trampolines)
}

fn spawn_trampoline_symbol(target_symbol: &str, spawn: AstNodeKey) -> String {
    format!(
        "__beskid_spawn_entry_syntax_{}_g{}_n{}",
        target_symbol
            .chars()
            .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
            .collect::<String>(),
        spawn.generation.0,
        spawn.node.0,
    )
}

/// Resolve trampoline entries for every freestanding [`LambdaExpression`] in the syntax tree.
///
/// Capture-free lambdas emit a simple entry function. Capturing lambdas require
/// generation-safe allocate/store/root authority before the entry is emitted.
fn resolve_lambda_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    _symbols: &HashMap<DirectCallee, String>,
) -> Result<Vec<LambdaTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut lambdas = Vec::new();
    let mut visited = HashSet::new();
    for item in items {
        collect_lambda_nodes(db, item.key, &mut visited, &mut lambdas);
    }
    let mut trampolines = Vec::new();
    for lambda in lambdas {
        let Some(lambda_sig) =
            closure_signature(db, lambda).map_err(|error| emission_verification(error.to_string()))?
        else {
            continue;
        };
        let Some(mut signature) = spawn_target_signature(isa, lambda_sig.callable) else {
            continue;
        };
        // Collect closure captures if present.
        let closure_captures = {
            let Some(environment) =
                closure_environment(db, lambda).map_err(|error| emission_verification(error.to_string()))?
            else {
                continue;
            };
            if environment.captures.is_empty() {
                None
            } else {
                let Some(authority) = input.closure_lowering_authority(lambda, lambda) else {
                    continue;
                };
                let Some(captures) = authority
                    .plan
                    .captures
                    .iter()
                    .map(|field| {
                        Some(beskid_isle::InlineCaptureField {
                            local_slot: beskid_isle::LocalSlotId {
                                owner_node: field.capture.slot.owner.node.0,
                                index: field.capture.slot.index,
                            },
                            field_offset: u32::try_from(field.field_offset).ok()?,
                            pointer_map_index: field.pointer_map_index,
                            value_type: map_spawn_capture_type(isa, field.abi_type)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                Some(captures)
            }
        };
        if closure_captures.is_some() {
            signature.params.insert(0, AbiParam::new(isa.pointer_type()));
        }
        let symbol = format!("__beskid_lambda_entry_syntax_g{}_n{}", lambda.generation.0, lambda.node.0);
        trampolines.push(LambdaTrampoline {
            lambda,
            lambda_body: lambda_sig.body,
            target_signature: signature,
            closure_captures,
            symbol,
        });
    }
    Ok(trampolines)
}

fn collect_lambda_nodes(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    lambdas: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::LambdaExpression) {
        lambdas.push(key);
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_lambda_nodes(db, child, visited, lambdas);
        }
    }
}

fn collect_spawn_nodes(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    spawns: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::SpawnExpression) {
        spawns.push(key);
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_spawn_nodes(db, child, visited, spawns);
        }
    }
}

fn emit_spawn_trampoline(
    trampoline: &SpawnTrampoline,
    isa: &dyn TargetIsa,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let mut signature = Signature::new(isa.default_call_conv());
    signature.params.push(AbiParam::new(pointer));
    signature.returns.push(AbiParam::new(types::I64));
    let mut function = Function::with_name_signature(cranelift_codegen::ir::UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let environment = builder.block_params(entry)[0];
        let target_signature = builder.import_signature(trampoline.target_signature.clone());
        let target = builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(trampoline.target_symbol.as_bytes()),
            signature: target_signature,
            colocated: false,
            patchable: false,
        });
        let call = if trampoline.closure_captures.is_some() {
            builder.ins().call(target, &[environment])
        } else {
            builder.ins().call(target, &[])
        };
        let results = builder.inst_results(call).to_vec();
        let result = match results.as_slice() {
            [] => builder.ins().iconst(types::I64, 0),
            [value] if builder.func.dfg.value_type(*value) == types::I64 => *value,
            [value] if builder.func.dfg.value_type(*value).is_int() => builder.ins().sextend(types::I64, *value),
            _ => {
                return Err(emission_verification(format!(
                    "spawn trampoline target `{}` must return unit or an integer ABI value",
                    trampoline.target_symbol
                )));
            }
        };
        builder.ins().return_(&[result]);
        builder.finalize();
    }
    verify_function(&function, isa.flags()).map_err(|error| {
        emission_verification(format!("spawn trampoline `{}` verification failed: {error}", trampoline.symbol))
    })?;
    Ok(function)
}

fn map_spawn_capture_type(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
    match semantic {
        SemanticTypeId::BOOL | SemanticTypeId::U8 => Some(types::I8),
        SemanticTypeId::I32 | SemanticTypeId::CHAR => Some(types::I32),
        SemanticTypeId::I64 => Some(types::I64),
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING => Some(isa.pointer_type()),
        SemanticTypeId::F64 => Some(types::F64),
        _ => None,
    }
}

fn spawn_target_signature(isa: &dyn TargetIsa, item: ItemSignature) -> Option<Signature> {
    fn map(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
        Some(match semantic {
            SemanticTypeId::BOOL | SemanticTypeId::U8 => types::I8,
            SemanticTypeId::I32 => types::I32,
            SemanticTypeId::I64 => types::I64,
            SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING => isa.pointer_type(),
            SemanticTypeId::F64 => types::F64,
            SemanticTypeId::CHAR => types::I32,
            SemanticTypeId::UNIT | SemanticTypeId::NEVER => return None,
            _ => return None,
        })
    }

    let mut signature = Signature::new(isa.default_call_conv());
    signature.params.extend(
        item.parameters
            .iter()
            .copied()
            .map(|semantic| map(isa, semantic).map(AbiParam::new))
            .collect::<Option<Vec<_>>>()?,
    );
    if !matches!(item.result, SemanticTypeId::UNIT | SemanticTypeId::NEVER) {
        signature.returns.push(AbiParam::new(map(isa, item.result)?));
    }
    Some(signature)
}

/// Trace only facts already read by the syntax-only lowering boundary.  This has no bearing on
/// selection; it makes every unavailable fact explicit instead of making a HIR-era guess.
fn trace_item_facts(input: &CodegenInput<'_>, item: AstNodeKey, symbols: &HashMap<DirectCallee, String>) {
    if !crate::isle_trace::enabled() {
        return;
    }
    let db = input.database();
    let mut visited = HashSet::new();
    trace_node_facts(db, item, symbols, &mut visited);
}

fn trace_node_facts(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    symbols: &HashMap<DirectCallee, String>,
    visited: &mut HashSet<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    let node = trace_key(db, key);
    let kind =
        node_kind(db, key).ok().flatten().map(|kind| format!("{kind:?}")).unwrap_or_else(|| "<missing>".to_owned());
    let span = node_span(db, key)
        .ok()
        .flatten()
        .map(|span| {
            format!(
                "{}:{}-{}:{} bytes={}-{}",
                span.line_col_start.0,
                span.line_col_start.1,
                span.line_col_end.0,
                span.line_col_end.1,
                span.start,
                span.end
            )
        })
        .unwrap_or_else(|| "<missing>".to_owned());
    crate::isle_trace::event(|| format!("event=ast.node key={node} kind={kind} span={span}"));

    if let Ok(Some(lowering)) = call_lowering(db, key) {
        let (lowering_name, callee) = match lowering {
            CallLowering::Direct(declaration) => {
                let callee = generic_call_specialization(db, key)
                    .ok()
                    .flatten()
                    .map(|specialization| {
                        DirectCallee::specialized_item(
                            specialization.declaration,
                            generic_specialization_identity(&GenericSpecializationInstance {
                                declaration: specialization.declaration,
                                signature: specialization.signature.clone(),
                                substitutions: specialization.substitutions.clone(),
                            }),
                        )
                    })
                    .unwrap_or_else(|| DirectCallee::item(declaration));
                ("Direct", Some(callee))
            }
            CallLowering::Dynamic => ("Dynamic", None),
            CallLowering::Runtime(intrinsic) => ("Runtime", Some(DirectCallee::runtime_intrinsic(intrinsic.0))),
            CallLowering::CorelibService(service) => {
                ("CorelibService", Some(DirectCallee::corelib_service(service.symbol)))
            }
        };
        match callee {
            Some(callee) => {
                let import = symbols.get(&callee).map(String::as_str).unwrap_or("<missing>");
                let callee_display = format_callee_for_trace(db, &callee);
                crate::isle_trace::event(|| {
                    format!(
                        "event=call.fact key={node} lowering={lowering_name} callee={callee_display} module_import={import}"
                    )
                });
            }
            None => crate::isle_trace::event(|| {
                format!("event=call.fact key={node} lowering={lowering_name} callee=<unavailable> module_import=<none>")
            }),
        }
    }

    match child_nodes(db, key) {
        Ok(Some(children)) => {
            for child in children.iter().copied() {
                trace_node_facts(db, child, symbols, visited);
            }
        }
        Ok(None) => {
            crate::isle_trace::event(|| format!("event=isle.missing rule=child_nodes key={node} detail=unavailable"))
        }
        Err(error) => {
            crate::isle_trace::event(|| format!("event=isle.missing rule=child_nodes key={node} detail={error}"))
        }
    }
}

fn trace_key(db: &dyn beskid_queries::Db, key: AstNodeKey) -> String {
    format_ast_node_key(db, key)
}

fn format_abi_identity(identity: &[u32]) -> String {
    if identity.is_empty() {
        return "[]".to_owned();
    }
    let (parameters, result) = identity.split_at(identity.len() - 1);
    let parameters =
        parameters.iter().copied().map(|id| SemanticTypeId(id).display_name()).collect::<Vec<_>>().join(", ");
    let result =
        result.first().copied().map(|id| SemanticTypeId(id).display_name()).unwrap_or_else(|| "unit".to_owned());
    format!("[{parameters}]->{result}")
}

fn format_declaration_for_trace(db: &dyn beskid_queries::Db, key: AstNodeKey) -> String {
    let name = item_name(db, key).ok().flatten().map(|name| name.to_string()).unwrap_or_else(|| "<anon>".to_owned());
    format!("{name}@{}", trace_key(db, key))
}

fn format_callee_for_trace(db: &dyn beskid_queries::Db, callee: &DirectCallee) -> String {
    match callee {
        DirectCallee::Item(key) => format!("Item({})", format_declaration_for_trace(db, *key)),
        DirectCallee::SpecializedItem { declaration, abi_identity } => format!(
            "SpecializedItem({}, {})",
            format_declaration_for_trace(db, *declaration),
            format_abi_identity(abi_identity)
        ),
        DirectCallee::RuntimeIntrinsic(index) => format!("RuntimeIntrinsic({index})"),
        DirectCallee::CorelibService(symbol) => format!("CorelibService({symbol})"),
        DirectCallee::SpawnTrampoline(spawn) => {
            format!("SpawnTrampoline({})", trace_key(db, *spawn))
        }
        DirectCallee::LambdaTrampoline(lambda) => {
            format!("LambdaTrampoline({})", trace_key(db, *lambda))
        }
    }
}

fn resolve_module_items(
    input: &CodegenInput<'_>,
    source_items: &[SyntaxModuleItem],
) -> Result<Vec<ResolvedSyntaxModuleItem>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut specializations = HashMap::<AstNodeKey, Vec<GenericSpecializationInstance>>::new();
    for item in source_items {
        // A generic declaration body has no concrete substitution environment of its own.
        // Walking it once here falsely treats `T`-dependent call sites as executable source and
        // can reject a program before a real direct-call instantiation reaches it. Only concrete
        // entry items seed the collection; each emitted generic item is represented solely by a
        // call-derived `DirectCallee::SpecializedItem` identity below.
        if is_concrete_executable_item(db, item.key)? {
            collect_generic_call_specializations(db, item.key, &mut specializations).map_err(|error| {
                emission_verification(format!(
                    "generic specialization collection failed for {}: {error}",
                    format_declaration_for_trace(db, item.key)
                ))
            })?;
        }
    }
    // Also collect specializations from entry-point roots (test files) that may
    // call generic functions defined in this module with concrete type arguments.
    for root in input.roots() {
        collect_generic_call_specializations(db, *root, &mut specializations).map_err(|error| {
            emission_verification(format!(
                "generic specialization collection failed for root {}: {error}",
                format_declaration_for_trace(db, *root)
            ))
        })?;
    }

    let mut resolved = Vec::with_capacity(source_items.len());
    for item in source_items {
        if item_abi_signature(db, item.key).ok().flatten().is_some() {
            resolved.push(ResolvedSyntaxModuleItem {
                key: item.key,
                symbol: item.symbol.clone(),
                callee: DirectCallee::item(item.key),
                specialization: None,
            });
            continue;
        }
        let kind = node_kind(db, item.key).map_err(|error| emission_verification(error.to_string()))?;
        if kind != Some(beskid_queries::IndexedNodeKind::FunctionDefinition) {
            // Type and enum declarations carry source layout facts but have no executable
            // syntax body. They deliberately do not require a call-derived function ABI.
            continue;
        }
        let Some(signatures) = specializations.get(&item.key) else {
            // Generic declarations do not have an executable ABI on their own.  They enter a
            // module only when the same source traversal has proven a concrete direct-call ABI.
            // A direct generic call without that proof is rejected while collecting below, so
            // this is only an uncalled declaration (for example a Corelib helper outside the
            // selected entrypoint's call graph).
            continue;
        };
        for specialization in signatures {
            let identity = generic_specialization_identity(specialization);
            resolved.push(ResolvedSyntaxModuleItem {
                key: item.key,
                symbol: format!("{}#generic_{}", item.symbol, specialization_mangle(specialization)),
                callee: DirectCallee::specialized_item(item.key, identity),
                specialization: Some(specialization.clone()),
            });
        }
    }
    Ok(resolved)
}

/// Return true only for a declaration body with a declaration-level, non-generic ABI.
///
/// Absence of an ABI is *not* treated as generic generally: non-function syntax declarations
/// are structural and have no executable body. Keeping this predicate explicit is the boundary
/// that prevents the specialization collector from scanning generic source as concrete code.
fn is_concrete_executable_item(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
) -> Result<bool, SyntaxModuleEmissionError> {
    Ok(item_abi_signature(db, key)
        .map_err(|error| {
            emission_verification(format!(
                "item ABI signature is unavailable for {}: {error}",
                format_declaration_for_trace(db, key)
            ))
        })?
        .is_some())
}

fn collect_generic_call_specializations(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    specializations: &mut HashMap<AstNodeKey, Vec<GenericSpecializationInstance>>,
) -> Result<(), SyntaxModuleEmissionError> {
    collect_generic_call_specializations_in_environment(db, key, None, specializations)
}

/// Traverse an executable source body with an optional immutable enclosing specialization.
/// Nested calls of the explicit `inner<T>(...)` form are materialized using the enclosing
/// bindings, then their body is queued in the same pass.  This replaces the old diagnostic-only
/// guard with a finite `(declaration, substitutions)` worklist.
fn collect_generic_call_specializations_in_environment(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    environment: Option<&GenericSpecializationInstance>,
    specializations: &mut HashMap<AstNodeKey, Vec<GenericSpecializationInstance>>,
) -> Result<(), SyntaxModuleEmissionError> {
    if let Some(declaration) = direct_generic_call_declaration(db, key).map_err(|error| {
            emission_verification(format!(
                "generic call analysis failed at {}: {error}",
                format_ast_node_site(db, key)
            ))
    })? {
        let specialization = if let Some(template) =
            generic_call_template(db, key).map_err(|error| emission_verification(error.to_string()))?
        {
            let enclosing = environment.ok_or_else(|| {
                emission_verification(format!(
                    "generic call template has no enclosing specialization: call={} declaration={}",
                    trace_key(db, key),
                    format_declaration_for_trace(db, declaration)
                ))
            })?;
            let bindings = template
                .parameters
                .iter()
                .zip(template.parameter_arguments.iter())
                .map(|(target, argument)| {
                    enclosing
                        .substitutions
                        .iter()
                        .find(|binding| binding.parameter.as_ref() == argument.as_ref())
                        .cloned()
                        .map(|binding| GenericSubstitution { parameter: target.clone(), argument: binding.argument })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    emission_verification(format!(
                        "nested generic call references an unbound parameter: call={} declaration={}",
                        trace_key(db, key),
                        format_declaration_for_trace(db, declaration)
                    ))
                })?;
            generic_specialization_instance(db, template.declaration, bindings.into())
                .map_err(|error| emission_verification(error.to_string()))?
                .ok_or_else(|| emission_verification("nested generic specialization is unavailable"))?
        } else {
            let specialization = generic_call_specialization(db, key)
                .map_err(|error| emission_verification(error.to_string()))?
                .ok_or_else(|| {
                    emission_verification(format!(
                        "generic direct call has no provable ABI specialization: call={} declaration={}",
                        trace_key(db, key),
                        format_declaration_for_trace(db, declaration)
                    ))
                })?;
            GenericSpecializationInstance {
                declaration: specialization.declaration,
                signature: specialization.signature,
                substitutions: specialization.substitutions,
            }
        };
        if specialization.declaration != declaration {
            return Err(emission_verification(format!(
                "generic direct call specialization resolved a different declaration: call={} expected={} actual={}",
                trace_key(db, key),
                format_declaration_for_trace(db, declaration),
                format_declaration_for_trace(db, specialization.declaration),
            )));
        }
        let instances = specializations.entry(declaration).or_default();
        if !instances.contains(&specialization) {
            instances.push(specialization.clone());
            // Only a newly discovered instance can add new nested work. This makes recursive
            // generic helpers finite without relying on a source-body traversal order.
            collect_generic_call_specializations_in_environment(
                db,
                declaration,
                Some(&specialization),
                specializations,
            )?;
        }
    }
    if let Some(children) = child_nodes(db, key).map_err(|error| emission_verification(error.to_string()))? {
        for child in children.iter().copied() {
            collect_generic_call_specializations_in_environment(db, child, environment, specializations)?;
        }
    }
    Ok(())
}

/// Returns the declaration only for a source call that has resolved directly to a generic
/// function with no declaration-level ABI.  This keeps module emission constrained to the same
/// semantic call facts that ISLE uses for `DirectCallee::SpecializedItem`; unresolved calls stay
/// unavailable rather than being guessed from syntax.
fn direct_generic_call_declaration(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
) -> Result<Option<AstNodeKey>, SyntaxModuleEmissionError> {
    if node_kind(db, key).map_err(|error| emission_verification(error.to_string()))?
        != Some(beskid_queries::IndexedNodeKind::CallExpression)
    {
        return Ok(None);
    }
    let lowering = match call_lowering(db, key) {
        Ok(Some(lowering)) => lowering,
        Ok(None) => return Ok(None),
        // `generic_call_specialization` deliberately leaves unavailable call sites out of the
        // fact set (for example unresolved Core.Output paths).  They cannot prove a direct
        // generic declaration and must not broaden module emission.
        Err(error) if error.is_unavailable() => return Ok(None),
        Err(error) => return Err(emission_verification(error.to_string())),
    };
    let CallLowering::Direct(declaration) = lowering else {
        return Ok(None);
    };
    if node_kind(db, declaration).map_err(|error| emission_verification(error.to_string()))?
        != Some(beskid_queries::IndexedNodeKind::FunctionDefinition)
    {
        return Ok(None);
    }
    match item_abi_signature(db, declaration).map_err(|error| emission_verification(error.to_string()))? {
        Some(_) => Ok(None),
        // Function definitions are ABI-less only when generic.  The generic call fact below
        // must now prove one exact ABI shape, otherwise the caller is rejected fail-closed.
        None => Ok(Some(declaration)),
    }
}

fn specialization_mangle(instance: &GenericSpecializationInstance) -> String {
    generic_specialization_identity(instance).iter().map(u32::to_string).collect::<Vec<_>>().join("_")
}

/// Syntax-ISLE adapter over the existing artifact-owned literal pool.
struct ArtifactStringInterner<'a> {
    context: &'a mut CodegenContext,
    pointer_type: Type,
}

impl StringInterner for ArtifactStringInterner<'_> {
    fn intern(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _key: AstNodeKey,
        text: &str,
    ) -> Result<Value, beskid_isle::StringMaterializationError> {
        let symbol = self.context.intern_string_literal(text.as_bytes());
        let global = builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(symbol),
            offset: 0.into(),
            colocated: true,
            tls: false,
        });
        let bytes = builder.ins().global_value(self.pointer_type, global);
        let byte_len = builder.ins().iconst(self.pointer_type, text.len() as i64);
        let route = beskid_abi::dispatch_route_for_symbol(beskid_abi::SYM_STR_NEW)
            .ok_or(beskid_isle::StringMaterializationError::MissingDispatchRoute(beskid_abi::SYM_STR_NEW))?;
        beskid_isle::emit_dispatch_call(builder, route, &[bytes, byte_len], true)
            .map_err(beskid_isle::StringMaterializationError::DispatchEmission)?
            .ok_or(beskid_isle::StringMaterializationError::DispatchEmission("str_new dispatch returned no value"))
    }
}

/// Manifest symbols available only to compiler-authorized canonical runtime source.  Ordinary
/// syntax programs never receive these entries, so an unresolved name cannot turn into an extern
/// fallback.
fn runtime_intrinsic_symbols(input: &CodegenInput<'_>) -> HashMap<DirectCallee, String> {
    input
        .runtime_intrinsic_capability()
        .map(|_| {
            input
                .abi_manifest()
                .trusted_runtime_intrinsics
                .iter()
                .enumerate()
                .filter_map(|(index, intrinsic)| {
                    u32::try_from(index)
                        .ok()
                        .map(|index| (DirectCallee::runtime_intrinsic(index), intrinsic.symbol.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn collect_extern_contract_callees(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    callees: &mut HashMap<DirectCallee, ExternImport>,
) {
    if let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, key)
        && let Some((symbol, abi, library)) = extern_contract_import_for_declaration(db, declaration)
    {
        callees.entry(DirectCallee::item(declaration)).or_insert(ExternImport { symbol, abi, library });
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_extern_contract_callees(db, child, callees);
        }
    }
}

fn extern_contract_symbols(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> HashMap<DirectCallee, String> {
    let mut callees = HashMap::new();
    for item in items {
        collect_extern_contract_callees(input.database(), item.key, &mut callees);
    }
    callees.into_iter().map(|(callee, import)| (callee, import.symbol)).collect()
}

fn extern_contract_imports(input: &CodegenInput<'_>, items: &[ResolvedSyntaxModuleItem]) -> Vec<ExternImport> {
    let mut callees = HashMap::new();
    for item in items {
        collect_extern_contract_callees(input.database(), item.key, &mut callees);
    }
    callees.into_values().collect()
}

/// ABI symbols admitted by the distinct Corelib syscall capability.  Unlike runtime intrinsics,
/// these imports can only be selected by a `CallLowering::CorelibService` syntax fact from the
/// exact embedded facade; ordinary dynamic calls never reach this table.
fn corelib_service_symbols(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> HashMap<DirectCallee, String> {
    let Some(capability) = input.corelib_service_capability() else {
        return HashMap::new();
    };
    let mut callees = HashSet::new();
    for item in items {
        collect_corelib_service_callees(input.database(), item.key, &mut callees);
    }
    capability
        .services()
        .iter()
        .filter(|service| callees.contains(&service.symbol))
        .map(|service| (DirectCallee::corelib_service(service.symbol), service.symbol.to_owned()))
        .collect()
}

fn collect_corelib_service_callees(db: &dyn beskid_queries::Db, key: AstNodeKey, callees: &mut HashSet<&'static str>) {
    if let Ok(Some(CallLowering::CorelibService(service))) = call_lowering(db, key) {
        callees.insert(service.symbol);
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_corelib_service_callees(db, child, callees);
        }
    }
}

struct ArtifactCallImporter<'a> {
    symbols: &'a HashMap<DirectCallee, String>,
}

impl beskid_isle::CallImporter for ArtifactCallImporter<'_> {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, beskid_isle::CallImportError> {
        let symbol = self.symbols.get(&callee).ok_or(beskid_isle::CallImportError::UnknownCallee)?;
        let signature = builder.import_signature(signature.clone());
        Ok(builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(symbol.as_bytes()),
            signature,
            colocated: false,
            patchable: false,
        }))
    }
}

/// Declare every syntax item before lowering any body, then import direct callees by exact
/// generation-safe call identity. This is the production module boundary for syntax → ISLE
/// lowering, including distinct generic instantiations of one source declaration.
pub fn emit_syntax_program<M: Module>(
    module: &mut M,
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[SyntaxModuleItem],
    linkage: Linkage,
) -> Result<HashMap<DirectCallee, FuncId>, SyntaxModuleEmissionError> {
    let items = expand_direct_spawn_items(input, resolve_module_items(input, items)?)?;
    let artifact = lower_resolved_syntax_program(input, isa, &items)?;
    for plan in &artifact.closure_static_plans {
        emit_closure_static_data(module, plan)?;
    }
    for plan in &artifact.aggregate_static_plans {
        emit_aggregate_static_data(module, plan)?;
    }
    for plan in &artifact.array_static_plans {
        emit_array_static_data(module, plan)?;
    }
    let mut by_callee = HashMap::with_capacity(items.len());
    let mut by_symbol = HashMap::with_capacity(artifact.functions.len());
    for lowered in &artifact.functions {
        let item_linkage =
            if lowered.name.starts_with("__beskid_spawn_entry_syntax_") { Linkage::Local } else { linkage };
        let id = module.declare_function(&lowered.name, item_linkage, &lowered.function.signature)?;
        by_symbol.insert(lowered.name.clone(), id);
    }
    for item in &items {
        let id = by_symbol[&item.symbol];
        by_callee.insert(item.callee.clone(), id);
    }
    crate::cranelift_host::declare_validated_extern_imports(module, &artifact, &mut by_symbol)
        .map_err(|error| SyntaxModuleEmissionError::Module(ModuleError::Backend(error.into())))?;
    for lowered in artifact.functions {
        let id = by_symbol[&lowered.name];
        let mut context = module.make_context();
        context.func = lowered.function;
        crate::cranelift_host::remap_testcase_externals(module, &mut context, &by_symbol)
            .map_err(|error| SyntaxModuleEmissionError::Module(ModuleError::Backend(error.into())))?;
        module.define_function(id, &mut context)?;
        module.clear_context(&mut context);
    }
    Ok(by_callee)
}

/// Emit source into a reusable module session.
///
/// The session namespaces both function and source-static identities before ordinary lowering.
/// Its artifact key is the complete ordered source item list, so a repeat request is served from
/// the cached `DirectCallee` handles and does not attempt a second Cranelift declaration.
pub fn emit_syntax_program_in_session<M: Module>(
    module: &mut M,
    session: &mut ModuleEmissionSession,
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[SyntaxModuleItem],
    linkage: Linkage,
) -> Result<HashMap<DirectCallee, FuncId>, SyntaxModuleEmissionError> {
    let namespace = session
        .namespace()
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect::<String>();
    let resolved = resolve_module_items(input, items)?;
    // Never cache by surface symbol alone: the same user symbol may represent a different AST
    // generation or a generic instance with ABI-equal but semantically different substitutions.
    let artifact_key = format!(
        "{namespace}:linkage={linkage:?}:{}",
        resolved
            .iter()
            .map(|item| format!("{:?}:{}:{:?}", item.key, item.symbol, item.callee))
            .collect::<Vec<_>>()
            .join(",")
    );
    if session.source_artifacts.contains(&artifact_key) {
        return resolved
            .into_iter()
            .map(|item| {
                session
                    .callees
                    .get(&item.callee)
                    .copied()
                    .map(|id| (item.callee, id))
                    .ok_or_else(|| emission_verification("module emission session cache is missing a declared callee"))
            })
            .collect();
    }
    let namespaced_input = input.with_artifact_namespace(session.namespace.clone());
    let namespaced_items = items
        .iter()
        .map(|item| SyntaxModuleItem { key: item.key, symbol: format!("__beskid_{namespace}_{}", item.symbol) })
        .collect::<Vec<_>>();
    let emitted = emit_syntax_program(module, &namespaced_input, isa, &namespaced_items, linkage)?;
    session.callees.extend(emitted.iter().map(|(callee, id)| (callee.clone(), *id)));
    session.source_artifacts.insert(artifact_key);
    Ok(emitted)
}

/// Emit artifact-owned closure descriptor/pointer-map/allocation-request data.
pub fn emit_closure_static_plans<M: Module>(module: &mut M, artifact: &CodegenArtifact) -> ModuleResult<()> {
    for plan in &artifact.closure_static_plans {
        emit_closure_static_data(module, plan)?;
    }
    for plan in &artifact.aggregate_static_plans {
        emit_aggregate_static_data(module, plan)?;
    }
    for plan in &artifact.array_static_plans {
        emit_array_static_data(module, plan)?;
    }
    Ok(())
}

/// Define one module-local data object per entry in `artifact.string_literals`.
pub fn emit_string_literals<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
) -> ModuleResult<HashMap<String, DataId>> {
    let mut handles = HashMap::new();
    for (symbol, data) in &artifact.string_literals {
        let data_id = module.declare_data(symbol, Linkage::Local, false, false)?;
        let mut ctx = DataDescription::new();
        ctx.define(data.clone().into_boxed_slice());
        module.define_data(data_id, &ctx)?;
        handles.insert(symbol.clone(), data_id);
    }
    Ok(handles)
}

/// Emit descriptor and offset-table data for every type in `artifact.type_descriptors`.
pub fn emit_type_descriptors<M: Module>(
    module: &mut M,
    artifact: &CodegenArtifact,
) -> ModuleResult<HashMap<TypeId, DescriptorHandles>> {
    let mut handles = HashMap::new();
    for (type_id, descriptor) in &artifact.type_descriptors {
        let offsets_id = declare_descriptor_offsets(module, *type_id)?;
        let offsets_ctx = build_offsets_data(module, descriptor);
        module.define_data(offsets_id, &offsets_ctx)?;

        let descriptor_id = declare_descriptor(module, *type_id)?;
        let descriptor_ctx = build_descriptor_data(module, descriptor, offsets_id);
        module.define_data(descriptor_id, &descriptor_ctx)?;

        handles.insert(*type_id, DescriptorHandles { descriptor: descriptor_id, offsets: offsets_id });
    }
    Ok(handles)
}

pub(crate) fn descriptor_offsets_symbol_name(type_id: TypeId) -> String {
    format!("__beskid_type_offsets_{}", type_id.0)
}

pub(crate) fn descriptor_symbol_name(type_id: TypeId) -> String {
    format!("__beskid_type_desc_{}", type_id.0)
}

fn declare_descriptor_offsets<M: Module>(module: &mut M, type_id: TypeId) -> ModuleResult<DataId> {
    let name = descriptor_offsets_symbol_name(type_id);
    module.declare_data(&name, Linkage::Local, false, false)
}

fn declare_descriptor<M: Module>(module: &mut M, type_id: TypeId) -> ModuleResult<DataId> {
    let name = descriptor_symbol_name(type_id);
    module.declare_data(&name, Linkage::Local, false, false)
}

fn build_offsets_data<M: Module>(module: &M, descriptor: &TypeDescriptorData) -> DataDescription {
    let mut ctx = DataDescription::new();
    let ptr_size = module.isa().pointer_bytes();
    let little_endian = matches!(module.isa().endianness(), Endianness::Little);

    let mut bytes = Vec::with_capacity(descriptor.pointer_offsets.len() * ptr_size as usize);
    for offset in &descriptor.pointer_offsets {
        write_usize(&mut bytes, *offset, ptr_size, little_endian);
    }
    ctx.define(bytes.into_boxed_slice());
    ctx
}

fn build_descriptor_data<M: Module>(
    module: &mut M,
    descriptor: &TypeDescriptorData,
    offsets_id: DataId,
) -> DataDescription {
    let ptr_size = module.isa().pointer_bytes();
    let little_endian = matches!(module.isa().endianness(), Endianness::Little);
    let usize_align = ptr_size as usize;
    let u32_align = 4usize;

    let mut ctx = DataDescription::new();
    let mut bytes = Vec::new();

    let _size_offset = push_usize(&mut bytes, descriptor.size, ptr_size, little_endian, usize_align);
    let _align_offset = push_usize(&mut bytes, descriptor.align, ptr_size, little_endian, usize_align);
    let _ptr_count_offset = push_u32(&mut bytes, descriptor.pointer_offsets.len() as u32, little_endian, u32_align);

    pad_to_alignment(&mut bytes, usize_align);
    let ptr_offsets_offset = bytes.len();
    bytes.extend(std::iter::repeat_n(0u8, usize_align));

    pad_to_alignment(&mut bytes, usize_align);
    let _name_offset = bytes.len();
    bytes.extend(std::iter::repeat_n(0u8, usize_align));

    ctx.define(bytes.into_boxed_slice());
    let gv = module.declare_data_in_data(offsets_id, &mut ctx);
    ctx.write_data_addr(ptr_offsets_offset as u32, gv, 0);
    ctx
}

fn write_usize(buf: &mut Vec<u8>, value: usize, ptr_size: u8, little_endian: bool) {
    match (ptr_size, little_endian) {
        (4, true) => buf.extend_from_slice(&(value as u32).to_le_bytes()),
        (4, false) => buf.extend_from_slice(&(value as u32).to_be_bytes()),
        (8, true) => buf.extend_from_slice(&(value as u64).to_le_bytes()),
        (8, false) => buf.extend_from_slice(&(value as u64).to_be_bytes()),
        _ => panic!("unsupported pointer size {ptr_size}"),
    }
}

fn push_usize(buf: &mut Vec<u8>, value: usize, ptr_size: u8, little_endian: bool, align: usize) -> usize {
    pad_to_alignment(buf, align);
    let offset = buf.len();
    write_usize(buf, value, ptr_size, little_endian);
    offset
}

fn push_u32(buf: &mut Vec<u8>, value: u32, little_endian: bool, align: usize) -> usize {
    pad_to_alignment(buf, align);
    let offset = buf.len();
    if little_endian {
        buf.extend_from_slice(&value.to_le_bytes());
    } else {
        buf.extend_from_slice(&value.to_be_bytes());
    }
    offset
}

fn pad_to_alignment(buf: &mut Vec<u8>, align: usize) {
    let padding = (align - (buf.len() % align)) % align;
    if padding > 0 {
        buf.extend(std::iter::repeat_n(0u8, padding));
    }
}
