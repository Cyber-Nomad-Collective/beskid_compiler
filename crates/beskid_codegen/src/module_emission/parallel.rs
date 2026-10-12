//! Parallel lowering of independent module items.
//!
//! Every source item lowers to its own CLIF function from read-only semantic facts, so items are
//! distributed over worker threads. Each worker reads through its own handle of the one Salsa
//! storage ([`beskid_queries::Db::fork_db`]); the handles observe the revision the input was
//! validated against, because nothing writes while emission runs. String literals go to one
//! shared pool whose symbols are content-addressed ([`crate::CodegenContext::intern_string_literal`]),
//! so they do not depend on emission order. Results are reassembled in item order and the first
//! failure in item order is reported, exactly as serial emission would.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use beskid_isle::{AstNodeKey, DirectCallee, StringInterner};
use cranelift_codegen::ir::Value;
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;

use super::contracts::{SyntaxModuleEmissionError, emission_error, emission_verification};
use super::imports::{ArtifactCallImporter, materialize_string_literal};
use super::items::ResolvedSyntaxModuleItem;
use super::trace::{trace_item_facts, trace_key};
use crate::{
    CodegenContext, CodegenInput, LoweredFunction, emit_isle_item_with_services,
    emit_isle_item_with_services_specialization,
};

/// Fewer items than this are lowered on the calling thread: forking handles and spawning workers
/// would cost more than it saves.
const PARALLEL_ITEM_THRESHOLD: usize = 32;
const MAX_WORKERS: usize = 16;

/// The literal pool shared by every item emitter of one artifact.
struct SharedStringInterner<'a, 'c> {
    pool: &'a Mutex<&'c mut CodegenContext>,
    pointer_type: cranelift_codegen::ir::Type,
}

impl StringInterner for SharedStringInterner<'_, '_> {
    fn intern(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _key: AstNodeKey,
        text: &str,
    ) -> Result<Value, beskid_isle::StringMaterializationError> {
        let symbol = self.pool.lock().expect("string literal pool").intern_string_literal(text.as_bytes());
        materialize_string_literal(builder, symbol, text, self.pointer_type)
    }
}

/// Number of emission workers: `BESKID_CODEGEN_THREADS` when set (1 forces serial emission),
/// otherwise the available parallelism, bounded by [`MAX_WORKERS`] and the item count.
fn worker_count(items: usize) -> usize {
    if items < PARALLEL_ITEM_THRESHOLD {
        return 1;
    }
    let requested = std::env::var("BESKID_CODEGEN_THREADS").ok().and_then(|value| value.parse::<usize>().ok());
    let available = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    requested.unwrap_or(available).clamp(1, MAX_WORKERS).min(items)
}

/// Lower every module item, in parallel when the module is large enough, returning the functions
/// in item order.
pub(super) fn emit_module_items(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
    context: &mut CodegenContext,
) -> Result<Vec<LoweredFunction>, SyntaxModuleEmissionError> {
    let pool = Mutex::new(context);
    let workers = worker_count(items.len());
    if workers <= 1 {
        return items.iter().map(|item| emit_module_item(input, isa, item, items, symbols, &pool)).collect();
    }
    let next = AtomicUsize::new(0);
    let first_error = AtomicUsize::new(usize::MAX);
    let mut slots: Vec<Option<Result<LoweredFunction, SyntaxModuleEmissionError>>> =
        (0..items.len()).map(|_| None).collect();
    let stack_size = beskid_pipeline::compiler_stack_size();
    std::thread::scope(|scope| -> Result<(), SyntaxModuleEmissionError> {
        let mut spawned = Vec::with_capacity(workers);
        for _ in 0..workers {
            // A `&dyn Db` cannot cross threads; an owned handle of the same storage can.
            let handle = input.database().fork_db();
            let detached = input.detach();
            let (pool, next, first_error) = (&pool, &next, &first_error);
            let worker = std::thread::Builder::new()
                .name("beskid-codegen".into())
                .stack_size(stack_size)
                .spawn_scoped(scope, move || {
                    let local = detached.attach(&*handle);
                    let mut emitted = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        // Indices are handed out in increasing order and `first_error` only
                        // decreases, so every index before the final first error is emitted.
                        if index >= items.len() || index > first_error.load(Ordering::Relaxed) {
                            break;
                        }
                        let result = emit_module_item(&local, isa, &items[index], items, symbols, pool);
                        if result.is_err() {
                            first_error.fetch_min(index, Ordering::Relaxed);
                        }
                        emitted.push((index, result));
                    }
                    emitted
                })
                .map_err(|error| emission_verification(format!("spawn code emission worker: {error}")))?;
            spawned.push(worker);
        }
        for worker in spawned {
            match worker.join() {
                Ok(emitted) => {
                    for (index, result) in emitted {
                        slots[index] = Some(result);
                    }
                }
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        Ok(())
    })?;
    let mut functions = Vec::with_capacity(items.len());
    for slot in slots {
        match slot {
            Some(Ok(function)) => functions.push(function),
            Some(Err(error)) => return Err(error),
            None => return Err(emission_verification("parallel code emission skipped an item before any failure")),
        }
    }
    Ok(functions)
}

/// Lower one module item: a compiler-generated bridge when the item is one, otherwise its ISLE body.
fn emit_module_item(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: &ResolvedSyntaxModuleItem,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
    pool: &Mutex<&mut CodegenContext>,
) -> Result<LoweredFunction, SyntaxModuleEmissionError> {
    let lowered = |function| LoweredFunction { name: item.symbol.clone(), function };
    let mut strings = SharedStringInterner { pool, pointer_type: isa.pointer_type() };
    if let Some(function) =
        super::serialization::emit_compiled_shape_bridge(input, isa, item, items, symbols, &mut strings)?
    {
        return Ok(lowered(function));
    }
    if let Some(function) = super::dynamic::emit_unpack_bridge(input, isa, item, items, symbols)? {
        return Ok(lowered(function));
    }
    if let Some(function) = super::dynamic::emit_shape_bridge(input, isa, item, items)? {
        return Ok(lowered(function));
    }
    if let Some(function) = super::dynamic::emit_pack_bridge(input, isa, item, items, symbols)? {
        return Ok(lowered(function));
    }
    trace_item_facts(input, item.key, symbols);
    let started = Instant::now();
    crate::isle_trace::event(|| {
        format!(
            "event=isle.selected rule=emit_item_statement item={} symbol={}",
            trace_key(input.database(), item.key),
            item.symbol
        )
    });
    let mut importer = ArtifactCallImporter { symbols };
    let function = match &item.specialization {
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
    })?;
    crate::isle_trace::event(|| {
        format!(
            "event=isle.emitted item={} elapsed_ms={}",
            trace_key(input.database(), item.key),
            started.elapsed().as_millis()
        )
    });
    Ok(lowered(function))
}
