use std::collections::{HashMap, HashSet};

use beskid_isle::AstNodeKey;
use beskid_queries::{child_nodes, closure_call_target, event_handler_lambda_for_local, spawn_entry_validation};
use cranelift_codegen::ir::Endianness;
use cranelift_module::{DataDescription, DataId, Linkage, Module, ModuleResult};

use super::{
    items::ResolvedSyntaxModuleItem,
    trampolines::{LambdaTrampoline, SpawnTrampoline},
};
use crate::{
    CodegenArtifact, CodegenInput,
    aggregate_static::{AggregateStaticPlan, emit_aggregate_static_data},
    array_static::{ArrayStaticPlan, emit_array_static_data},
    closure_static::{ClosureStaticPlan, emit_closure_static_data},
};

pub(super) fn collect_array_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Vec<ArrayStaticPlan> {
    let mut symbols = HashSet::new();
    let mut plans = Vec::new();
    for item in items {
        let mut visited = HashSet::new();
        let mut nodes = Vec::new();
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
        for key in nodes {
            let plan = input
                .array_static_plan_for_specialization(key, item.specialization.as_ref())
                .or_else(|| input.bulk_array_static_plan(key, item.specialization.as_ref()))
                .or_else(|| input.typed_array_static_plan(key, item.specialization.as_ref()))
                .or_else(|| input.canonical_empty_statics(key, item.specialization.as_ref())?.array);
            if let Some(plan) = plan
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                plans.push(plan);
            }
        }
    }
    if let Some((composition, _)) = input.composition_authority() {
        for plural in &composition.plurals {
            let field = AstNodeKey {
                unit: input.typed_program().entry,
                generation: input.typed_program().generation,
                node: plural.field_node_id,
            };
            if let Some(plan) = input.composition_plural_static_plan(field, plural.target_slots.len())
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                plans.push(plan);
            }
        }
    }
    plans
}

pub(super) fn collect_aggregate_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Vec<AggregateStaticPlan> {
    let mut symbols = HashSet::new();
    let mut plans = Vec::new();
    for item in items {
        let mut visited = HashSet::new();
        let mut nodes = Vec::new();
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
        for key in nodes {
            let plan = input
                .aggregate_static_plan_for_specialization(key, item.specialization.as_ref())
                .or_else(|| input.enum_static_plan_for_specialization(key, item.specialization.as_ref()))
                .or_else(|| input.canonical_empty_statics(key, item.specialization.as_ref())?.object);
            if let Some(plan) = plan
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                plans.push(plan);
            }
        }
    }
    if let Some((composition, _)) = input.composition_authority() {
        for activation in &composition.activation {
            if let Some(plan) = input.composition_registration_static_plan(activation.registration_id)
                && symbols.insert(plan.allocation_request_symbol.clone())
            {
                plans.push(plan);
            }
        }
    }
    plans
}

/// Collect source-proven closure static plans from generation-safe syntax facts.
///
/// Direct items and capture-free lambdas each receive syntax-owned trampoline targets. Capturing
/// lambdas require generation-safe allocate/store/root authority before a trampoline is emitted.
pub(super) fn collect_closure_static_plans(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    trampolines: &[SpawnTrampoline],
    lambda_trampolines: &[LambdaTrampoline],
) -> Vec<ClosureStaticPlan> {
    let db = input.database();
    let mut plans = Vec::new();
    let mut seen = HashSet::new();
    // One lambda of a generic body has one plan per enclosing instance, told apart by symbol.
    let mut push_plan = |plan: ClosureStaticPlan| {
        if seen.insert(plan.descriptor_symbol.clone()) {
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
            && let Some(instance) = trampoline.specialization.as_ref()
        {
            if let Some(authority) =
                input.specialized_closure_lowering_authority(trampoline.lambda, trampoline.lambda, instance)
            {
                push_plan(authority.plan);
            }
            continue;
        }
        if trampoline.closure_captures.is_some() {
            let event_handler = event_handler_lambda_for_local(db, trampoline.lambda).ok().flatten().is_some();
            let authority = if event_handler {
                input.event_handler_closure_lowering_authority(trampoline.lambda, trampoline.lambda)
            } else {
                input.closure_lowering_authority(trampoline.lambda, trampoline.lambda)
            };
            if let Some(authority) = authority {
                push_plan(authority.plan);
            }
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

pub(super) fn event_handler_wrapper_required(input: &CodegenInput<'_>, items: &[ResolvedSyntaxModuleItem]) -> bool {
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    for item in items {
        collect_ast_nodes(input.database(), item.key, &mut visited, &mut nodes);
    }
    // Function values (materialized lambdas and named functions read as values) share the
    // event-handler closure record layout and allocation request.
    nodes.into_iter().any(|key| {
        matches!(
            beskid_queries::event_operation(input.database(), key),
            Ok(Some(fact)) if fact.handler_lambda.is_some()
        ) || matches!(beskid_queries::lambda_value_required(input.database(), key), Ok(Some(true)))
            || matches!(beskid_queries::function_item_value(input.database(), key), Ok(Some(_)))
    })
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

/// Declare the runtime kit data symbols that lowered functions read through a symbol global as
/// imports: today the write-barrier process word `beskid_rt_v5_gc_barrier_active`, which the
/// guarded barrier of a managed field or element store loads. The kit defines it; a module that
/// does not read it declares nothing.
pub(crate) fn declare_runtime_data_imports<M: Module>(module: &mut M, artifact: &CodegenArtifact) -> ModuleResult<()> {
    for function in &artifact.functions {
        declare_function_runtime_data_imports(module, &function.function)?;
    }
    Ok(())
}

/// Declare the runtime kit data imports one lowered function reads ([`declare_runtime_data_imports`]),
/// for bodies that are defined outside the artifact (checked-effect closures) before their symbol
/// globals are remapped. Declaring the same import twice yields the same data id.
pub(crate) fn declare_function_runtime_data_imports<M: Module>(
    module: &mut M,
    function: &cranelift_codegen::ir::Function,
) -> ModuleResult<()> {
    let referenced = function.global_values.values().any(|global| {
        matches!(
            global,
            cranelift_codegen::ir::GlobalValueData::Symbol {
                name: cranelift_codegen::ir::ExternalName::TestCase(name),
                ..
            } if name.raw() == beskid_isle::GC_BARRIER_ACTIVE_SYMBOL.as_bytes()
        )
    });
    if referenced {
        module.declare_data(beskid_isle::GC_BARRIER_ACTIVE_SYMBOL, Linkage::Import, false, false)?;
    }
    Ok(())
}

/// Emit artifact-owned closure descriptor/pointer-map/allocation-request data, after declaring the
/// runtime data symbols the artifact's functions import ([`declare_runtime_data_imports`]).
pub fn emit_closure_static_plans<M: Module>(module: &mut M, artifact: &CodegenArtifact) -> ModuleResult<()> {
    declare_runtime_data_imports(module, artifact)?;
    if artifact.event_handler_wrapper_required {
        crate::emit_event_handler_static_data(module)?;
    }
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
    // Declare in symbol order so the object layout does not follow hash-map iteration order.
    let mut literals = artifact.string_literals.iter().collect::<Vec<_>>();
    literals.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (symbol, data) in literals {
        let data_id = module.declare_data(symbol, Linkage::Local, false, false)?;
        let mut ctx = DataDescription::new();
        ctx.define(data.clone().into_boxed_slice());
        module.define_data(data_id, &ctx)?;
        handles.insert(symbol.clone(), data_id);
    }
    Ok(handles)
}

