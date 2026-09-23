//! Spawn and lambda entry trampolines, plus the scheduler fiber-entry/return glue.

mod emit;
mod import_helpers;
mod lambda;
mod scheduler;
mod spawn;
mod types;

pub(super) use emit::{conservative_fiber_stack_requirement, emit_spawn_trampoline};
pub(super) use lambda::resolve_lambda_trampolines;
pub(super) use scheduler::{emit_scheduler_fiber_entry, emit_scheduler_return_trampoline};
pub(super) use spawn::{expand_direct_spawn_items, resolve_spawn_trampolines};
pub(super) use types::{LambdaTrampoline, SchedulerCompletionTransfer, SpawnTrampoline};
