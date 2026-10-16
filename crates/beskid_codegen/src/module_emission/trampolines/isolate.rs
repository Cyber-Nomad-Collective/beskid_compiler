//! Isolate entry trampolines for `__isolate_spawn<T>` / `__isolate_spawn_local<T>`.
//!
//! The runtime adapters take `(entry, argument, scope)`. `argument` is the body's closure record
//! (after transit on the destination isolate) and `entry` is the trampoline for the ABI shape of
//! `T`: it calls the record's code (environment first when the record carries one) and boxes the
//! result exactly like a `spawn` trampoline, so the isolate's main fiber completes through the same
//! `FiberDone` result slot and `__isolate_join_value<T>` reads it like `fiber_join_value`.
//!
//! The trampoline depends only on the ABI shape of `T` (its semantic ABI type and whether the
//! value is a managed reference), so one artifact emits one trampoline per shape it spawns.

use std::collections::{BTreeMap, HashSet};

use beskid_isle::AstNodeKey;
use beskid_queries::{
    CallLowering, GenericSpecializationInstance, ManagedReferenceKind, SemanticTypeId, call_lowering,
};
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, Type, types};
use cranelift_codegen::ir::{BlockArg, MemFlagsData, TrapCode, condcodes::IntCC};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use super::emit::{emit_boxed_result, emit_entry_stack_guard};
use crate::CodegenInput;
use crate::aggregate_static::{AggregateStaticField, AggregateStaticPlan};
use crate::isle_adapter::mappings::map_signature_type;
use crate::module_emission::contracts::{SyntaxModuleEmissionError, emission_verification};
use crate::module_emission::items::ResolvedSyntaxModuleItem;

/// Usable fiber stack an isolate main fiber requests before it enters the body. The body is
/// reached through a closure record, so its frame is not known when the trampoline is emitted;
/// this is a fixed reserve of one megabyte (an isolate owns an OS thread, so the cost is bounded
/// by the isolate count).
const ISOLATE_ENTRY_STACK_RESERVE: u64 = 1 << 20;

/// Symbol prefix of isolate entry trampolines; they are local to their artifact.
pub(in crate::module_emission) const ISOLATE_ENTRY_SYMBOL_PREFIX: &str = "__beskid_isolate_entry_";

const ISOLATE_SPAWN_SERVICES: [&str; 2] = ["isolate_spawn", "isolate_spawn_local"];

#[derive(Debug, Clone)]
pub(in crate::module_emission) struct IsolateEntryTrampoline {
    pub(in crate::module_emission) result: SemanticTypeId,
    pub(in crate::module_emission) managed: bool,
    /// ABI type of the body's result; `None` for `unit`.
    pub(in crate::module_emission) value_type: Option<Type>,
    pub(in crate::module_emission) symbol: String,
    pub(in crate::module_emission) result_plan: AggregateStaticPlan,
}

/// The ABI shape of `T` for one `__isolate_spawn<T>` call read in `enclosing`. A native pointer
/// is not transferable and has no shape.
pub(crate) fn isolate_entry_shape(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<(SemanticTypeId, bool)> {
    let binding = beskid_queries::specialized_builtin_type_argument(db, key, enclosing).ok().flatten()?;
    let managed = binding.managed_reference_kind() == ManagedReferenceKind::GcManaged;
    if binding.argument == SemanticTypeId::POINTER && !managed {
        return None;
    }
    Some((binding.argument, managed))
}

/// One trampoline per result shape spawned by `items`, each body read in its own specialization.
pub(in crate::module_emission) fn resolve_isolate_entry_trampolines(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
) -> Result<Vec<IsolateEntryTrampoline>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut shapes = BTreeMap::new();
    for item in items {
        let mut visited = HashSet::new();
        let mut calls = Vec::new();
        collect_isolate_spawn_calls(db, item.key, &mut visited, &mut calls);
        for call in calls {
            let shape = isolate_entry_shape(db, call, item.specialization.as_ref()).ok_or_else(|| {
                emission_verification(format!(
                    "isolate spawn result type unavailable at {}",
                    beskid_queries::format_ast_node_key(db, call)
                ))
            })?;
            shapes.entry((shape.0.0, shape.1)).or_insert(call);
        }
    }
    let header = input
        .abi_manifest()
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidObjectHeader")
        .ok_or_else(|| emission_verification("isolate result object header unavailable"))?;
    shapes
        .into_iter()
        .map(|((result, managed), call)| {
            let result = SemanticTypeId(result);
            let value_type = if matches!(result, SemanticTypeId::UNIT | SemanticTypeId::NEVER) {
                None
            } else {
                Some(map_signature_type(isa, result).ok_or_else(|| {
                    emission_verification(format!("isolate result type {} has no ABI type", result.0))
                })?)
            };
            let symbol = isolate_entry_symbol(result, managed);
            let result_plan = AggregateStaticPlan {
                descriptor_flags: 0,
                descriptor_getter: None,
                literal: call,
                descriptor_symbol: format!("{symbol}_result_descriptor"),
                pointer_map_symbol: format!("{symbol}_result_pointer_map"),
                allocation_request_symbol: format!("{symbol}_result_request"),
                object_size: header.size + 8,
                object_alignment: header.alignment,
                pointer_map_offsets: if managed { vec![header.size] } else { Vec::new() }.into(),
                fields: vec![AggregateStaticField { abi_type: result, field_offset: header.size }].into(),
            };
            Ok(IsolateEntryTrampoline { result, managed, value_type, symbol, result_plan })
        })
        .collect()
}

pub(in crate::module_emission) fn isolate_entry_symbol(result: SemanticTypeId, managed: bool) -> String {
    format!("{ISOLATE_ENTRY_SYMBOL_PREFIX}t{}{}", result.0, if managed { "_managed" } else { "" })
}

fn collect_isolate_spawn_calls(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
    calls: &mut Vec<AstNodeKey>,
) {
    if !visited.insert(key) {
        return;
    }
    if beskid_queries::node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::CallExpression)
        && let Ok(Some(CallLowering::CorelibService(service))) = call_lowering(db, key)
        && ISOLATE_SPAWN_SERVICES.contains(&service.symbol)
    {
        calls.push(key);
    }
    if let Ok(Some(children)) = beskid_queries::child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_isolate_spawn_calls(db, child, visited, calls);
        }
    }
}

/// `i64 entry(pointer record)`: call the `() => T` record and return its boxed result.
pub(in crate::module_emission) fn emit_isolate_entry_trampoline(
    trampoline: &IsolateEntryTrampoline,
    isa: &dyn TargetIsa,
    scheduler_stack_check_symbol: &str,
    scheduler_stack_overflow_symbol: &str,
) -> Result<Function, SyntaxModuleEmissionError> {
    let pointer = isa.pointer_type();
    let call_conv = isa.default_call_conv();
    let mut signature = Signature::new(call_conv);
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
        let record = builder.block_params(entry)[0];
        emit_entry_stack_guard(
            &mut builder,
            pointer,
            ISOLATE_ENTRY_STACK_RESERVE,
            &trampoline.symbol,
            scheduler_stack_check_symbol,
            scheduler_stack_overflow_symbol,
        )?;
        // The runtime roots the record for the fiber's lifetime.
        builder.ins().trapz(record, TrapCode::unwrap_user(5));
        let code = builder.ins().load(pointer, MemFlagsData::new(), record, crate::EVENT_HANDLER_CODE_OFFSET);
        let environment =
            builder.ins().load(pointer, MemFlagsData::new(), record, crate::EVENT_HANDLER_ENVIRONMENT_OFFSET);
        let capture_free = builder.create_block();
        let captured = builder.create_block();
        let merge = builder.create_block();
        if let Some(value_type) = trampoline.value_type {
            builder.append_block_param(merge, value_type);
        }
        let has_environment = builder.ins().icmp_imm(IntCC::NotEqual, environment, 0);
        builder.ins().brif(has_environment, captured, &[], capture_free, &[]);

        builder.switch_to_block(capture_free);
        builder.seal_block(capture_free);
        let mut plain = Signature::new(call_conv);
        plain.returns.extend(trampoline.value_type.map(AbiParam::new));
        let plain = builder.import_signature(plain);
        let call = builder.ins().call_indirect(plain, code, &[]);
        let results: Vec<BlockArg> = builder.inst_results(call).iter().copied().map(BlockArg::Value).collect();
        builder.ins().jump(merge, &results);

        builder.switch_to_block(captured);
        builder.seal_block(captured);
        let mut closure = Signature::new(call_conv);
        closure.params.push(AbiParam::new(pointer));
        closure.returns.extend(trampoline.value_type.map(AbiParam::new));
        let closure = builder.import_signature(closure);
        let call = builder.ins().call_indirect(closure, code, &[environment]);
        let results: Vec<BlockArg> = builder.inst_results(call).iter().copied().map(BlockArg::Value).collect();
        builder.ins().jump(merge, &results);

        builder.switch_to_block(merge);
        builder.seal_block(merge);
        let result = match builder.block_params(merge).first().copied() {
            Some(value) => value,
            None => builder.ins().iconst(types::I64, 0),
        };
        let boxed = emit_boxed_result(&mut builder, pointer, result, &trampoline.result_plan);
        builder.ins().return_(&[boxed]);
        builder.finalize(isa.frontend_config());
    }
    verify_function(&function, isa.flags()).map_err(|error| {
        emission_verification(format!("isolate entry trampoline `{}` verification failed: {error}", trampoline.symbol))
    })?;
    Ok(function)
}
