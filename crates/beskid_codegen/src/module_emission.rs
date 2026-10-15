pub(crate) mod canonical_empty;
mod contracts;
pub(crate) mod data;
pub(crate) mod dynamic;
pub(crate) mod imports;
mod items;
mod optional_externs;
mod orchestration;
mod parallel;
mod runtime_checked;
mod specialization;
mod trace;
mod trampolines;

/// Canonical source callees invoked by compiler-generated scheduler entry/return trampolines.
/// They are explicit reachability roots because indirect context entry is not a source call.
pub(crate) const SCHEDULER_ENTRY_HELPERS: &[&str] = &[
    "SchedulerContext",
    "SchedulerSetCurrentFiber",
    "ContextSwitch",
    "SchedulerCurrentFiber",
    "FiberRecord",
    "FiberDone",
];

/// Canonical source seams invoked only by compiler-generated spawn trampolines. They are extra
/// reachability roots whenever a scheduler-backed compilation also contains a `spawn`.
pub(crate) const SCHEDULER_STACK_HELPERS: &[&str] = &["SchedulerStackCheck", "SchedulerStackOverflowObserved"];

pub use contracts::SyntaxModuleEmissionError;
pub use data::{emit_closure_static_plans, emit_string_literals};
pub use items::SyntaxModuleItem;
pub use optional_externs::OPTIONAL_EXTERN_ABSENT_SYMBOL;
pub use orchestration::{
    CheckedFailureEntry, ModuleEmissionSession, checked_failure_entries, checked_failure_plans, emit_syntax_program,
    emit_syntax_program_in_session, lower_syntax_program,
};

pub(crate) use orchestration::emitted_item_keys;

mod serialization;
