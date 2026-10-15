//! Runtime symbols of the 0.6.2 hybrid write barrier that generated code references.
//!
//! A store that overwrites a GC-managed pointer slot inside an existing managed object first
//! loads the process word [`GC_BARRIER_ACTIVE_SYMBOL`] (the number of heaps that are marking)
//! and, only when it is non-zero, calls [`GC_WRITE_BARRIER_SYMBOL`] `(slot, value)` in a cold
//! block. The barrier shades the value currently in `slot` and `value` when the calling heap is
//! marking; it does not allocate, collect, yield or trap, so it is not a safepoint
//! (execution/runtime/memory-and-gc-runtime-contract, "Hybrid write barrier" and "Write barrier
//! emission in generated code").

/// The hybrid barrier `(ptr slot, ptr value) -> unit`.
pub const GC_WRITE_BARRIER_SYMBOL: &str = "beskid_rt_v5_gc_write_barrier";

/// The one-word process data symbol that counts the heaps that are marking.
pub const GC_BARRIER_ACTIVE_SYMBOL: &str = "beskid_rt_v5_gc_barrier_active";
