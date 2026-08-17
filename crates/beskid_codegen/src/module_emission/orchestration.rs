use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use beskid_isle::DirectCallee;
use cranelift_codegen::isa::TargetIsa;
use cranelift_module::{FuncId, Linkage, Module, ModuleError};

use super::contracts::{SyntaxModuleEmissionError, emission_error, emission_verification};
use super::data::{collect_aggregate_static_plans, collect_array_static_plans, collect_closure_static_plans};
use super::imports::{
    ArtifactCallImporter, ArtifactStringInterner, corelib_service_symbols, extern_contract_imports,
    extern_contract_symbols, runtime_intrinsic_symbols,
};
use super::items::{ResolvedSyntaxModuleItem, SyntaxModuleItem};
use super::specialization::resolve_module_items;
use super::trace::{trace_item_facts, trace_key};
use super::trampolines::{
    conservative_fiber_stack_requirement, emit_scheduler_fiber_entry, emit_scheduler_return_trampoline,
    emit_spawn_trampoline, expand_direct_spawn_items, resolve_lambda_trampolines, resolve_spawn_trampolines,
};
use crate::aggregate_static::{ABI_V5_MANAGED_OBJECT_ALLOCATE, emit_aggregate_static_data};
use crate::array_static::{
    ABI_V5_ARRAY_ALLOCATE_ROOTED, ABI_V5_ARRAY_CONSTRUCTION_FINISH, ABI_V5_ARRAY_GROW_ROOTED, emit_array_static_data,
};
use crate::closure_static::{
    ABI_V5_CLOSURE_CAPTURE_STORE, ABI_V5_CLOSURE_ENVIRONMENT_ALLOCATE, ABI_V5_CLOSURE_ENVIRONMENT_ROOT_CURRENT,
    emit_closure_static_data,
};
use crate::{
    CodegenArtifact, CodegenContext, CodegenInput, ExternImport, emit_isle_closure_lambda_entry,
    emit_isle_expression_with_call_importer, emit_isle_item_with_services, emit_isle_item_with_services_specialization,
};

const ABI_V5_FIBER_SPAWN_WITH_CANCEL_SLOT: &str = "beskid_rt_v5_fiber_spawn_with_cancel_slot";

/// State owned by a long-lived Cranelift module while it receives source artifacts.
///
/// A session supplies one namespace for source-owned metadata and remembers function handles by
/// final symbol. Re-emitting the same source artifact therefore returns its existing handles
/// rather than redeclaring a conflicting module symbol. Callers that emit independent artifacts
/// choose distinct namespaces; the convenience API below keeps the historical one-shot behavior.
#[derive(Debug, Clone)]
pub struct ModuleEmissionSession {
    namespace: Arc<str>,
    callees: HashMap<DirectCallee, FuncId>,
    source_artifacts: HashSet<String>,
}

impl ModuleEmissionSession {
    pub fn new(namespace: impl Into<Arc<str>>) -> Self {
        Self { namespace: namespace.into(), callees: HashMap::new(), source_artifacts: HashSet::new() }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }
}

/// Lower typed syntax items through generated ISLE into the backend artifact boundary.
/// Direct calls retain their exact syntax item identity while emitted CLIF references the final
/// declared symbol by name.
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

    // The scheduler entry symbols (context, set/current fiber, context switch, fiber record) are
    // always required for the compiler-generated fiber entry and return trampolines. The stack
    // check and overflow seam functions are only invoked by spawn trampolines, so they are
    // resolved lazily: a canonical runtime corpus with no spawn expressions never reaches them
    // and must not require them to be reachable from manifest exports.
    let scheduler_symbols = if input.runtime_intrinsic_capability().is_some() {
        let symbol = |name: &str| {
            items
                .iter()
                .find(|item| {
                    beskid_queries::item_name(input.database(), item.key).ok().flatten().as_deref() == Some(name)
                })
                .map(|item| item.symbol.as_str())
        };
        let entry = (
            symbol("SchedulerContext")
                .ok_or_else(|| emission_verification("canonical SchedulerContext item unavailable"))?,
            symbol("SchedulerSetCurrentFiber")
                .ok_or_else(|| emission_verification("canonical SchedulerSetCurrentFiber item unavailable"))?,
            symbol("ContextSwitch").ok_or_else(|| emission_verification("canonical ContextSwitch item unavailable"))?,
            symbol("SchedulerCurrentFiber")
                .ok_or_else(|| emission_verification("canonical SchedulerCurrentFiber item unavailable"))?,
            symbol("FiberRecord").ok_or_else(|| emission_verification("canonical FiberRecord item unavailable"))?,
        );
        let stack = if trampolines.is_empty() {
            None
        } else {
            Some((
                symbol("SchedulerStackCheck")
                    .ok_or_else(|| emission_verification("canonical SchedulerStackCheck item unavailable"))?,
                symbol("SchedulerStackOverflowObserved").ok_or_else(|| {
                    emission_verification("canonical SchedulerStackOverflowObserved item unavailable")
                })?,
            ))
        };
        Some((entry, stack))
    } else {
        None
    };

    let mut context = CodegenContext::new_with_artifact_namespace(input.artifact_namespace().to_owned());
    let lambda_count =
        trampolines.iter().filter(|trampoline| trampoline.lambda_body.is_some()).count() + lambda_trampolines.len();
    let mut functions = Vec::with_capacity(
        items.len()
            + trampolines.len()
            + lambda_count
            + lambda_trampolines.len()
            + usize::from(scheduler_symbols.is_some()) * 2,
    );
    if let Some((entry, _)) = scheduler_symbols {
        let (scheduler_context, set_current, context_switch, current, fiber_record) = entry;
        functions.push(crate::LoweredFunction {
            name: "__beskid_scheduler_fiber_entry".to_owned(),
            function: emit_scheduler_fiber_entry(isa, scheduler_context, set_current, context_switch)?,
        });
        functions.push(crate::LoweredFunction {
            name: "__beskid_scheduler_return_trampoline".to_owned(),
            function: emit_scheduler_return_trampoline(
                isa,
                current,
                fiber_record,
                scheduler_context,
                set_current,
                context_switch,
            )?,
        });
    }
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
    if !trampolines.is_empty() {
        let Some((_, Some((stack_check, stack_overflow)))) = scheduler_symbols else {
            return Err(emission_verification("fiber stack checks require the exact canonical Scheduler corpus"));
        };
        for trampoline in &trampolines {
            let target =
                functions.iter().find(|function| function.name == trampoline.target_symbol).ok_or_else(|| {
                    emission_verification(format!(
                        "fiber target `{}` was not emitted before its stack bound",
                        trampoline.target_symbol
                    ))
                })?;
            let required = conservative_fiber_stack_requirement(&target.function, &trampoline.target_symbol)?;
            functions.push(crate::LoweredFunction {
                name: trampoline.symbol.clone(),
                function: emit_spawn_trampoline(trampoline, isa, required, stack_check, stack_overflow)?,
            });
        }
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
        for symbol in [
            ABI_V5_ARRAY_ALLOCATE_ROOTED,
            ABI_V5_ARRAY_GROW_ROOTED,
            ABI_V5_ARRAY_CONSTRUCTION_FINISH,
            "beskid_rt_v5_array_write_barrier",
        ] {
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
        let item_linkage = if lowered.name.starts_with("__beskid_spawn_entry_syntax_")
            || lowered.name.starts_with("__beskid_scheduler_")
        {
            Linkage::Local
        } else {
            linkage
        };
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
