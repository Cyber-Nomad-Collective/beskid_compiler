//! Runtime diagnostics and advanced GC controls for host tooling.

mod gc;

pub use gc::{GcSnapshot, collect_if_needed, force_collect, snapshot_gc, write_barrier};
