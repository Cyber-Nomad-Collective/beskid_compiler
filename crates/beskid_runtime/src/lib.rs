//! Runtime support for Beskid: Abfall-backed allocation, cooperative fibers, channels, and JIT TLS hooks.
//!
//! Symbols and layouts are shared with [`beskid_abi`] for codegen and linker registration.
//!
//! ## GC phases
//!
//! - **Phase A (default):** one Beskid mutator at a time, cooperative fibers swap on a single
//!   scheduler OS thread, channels carry `i64` payloads, syscall pool workers do not allocate.
//! - **Phase B (opt-in, off by default):** multiple OS-thread mutators may share one
//!   [`abfall::Heap`] concurrently. Pointer-payload channels ([`channel_send_ptr`] /
//!   [`channel_receive_ptr`]) anchor in-flight pointers in the heap's external root table and
//!   invoke the insertion write barrier on send and receive so concurrent marking retains
//!   reachability across fibers. Syscall pool workers are tagged ([`set_syscall_pool_worker`])
//!   and will panic on accidental allocation without [`enter_runtime_scope`]. Optional
//!   function-entry preemption is exposed through [`runtime_preempt_check`].
//!
//! Toggle Phase B via [`set_runtime_phase`] or the `BESKID_RUNTIME_PHASE_B=1` environment
//! variable; preemption is toggled by [`set_preemption_enabled`] / `BESKID_RUNTIME_PREEMPT=1`.

pub mod builtins;
pub mod channel;
pub mod composition;
pub mod fiber;
pub mod gc;
pub mod hub;
pub mod interop;
mod interop_layout;
pub mod mutex;
pub mod runtime;
pub mod scheduler;
mod slot_table;
pub mod status;
pub mod wait_group;

pub use builtins::{
    alloc, array_len, array_new, beskid_runtime_abi_version, channel_close, channel_create,
    channel_receive, channel_receive_status, channel_receive_value, channel_send,
    channel_try_receive, channel_try_send, composition_bind_plural, composition_container_create,
    composition_container_drop, composition_launch, composition_register, composition_resolve,
    composition_resolve_plural, composition_scope_depth, composition_scope_enter,
    composition_scope_leave, composition_shutdown, event_get_handler, event_len, event_subscribe,
    event_unsubscribe_first, fiber_cancel, fiber_current_id, fiber_detach, fiber_join,
    fiber_join_status, fiber_join_value, fiber_now_millis, fiber_processor_count, fiber_spawn,
    fiber_spawn_with_cancel_slot, fiber_yield, gc_bytes_allocated, gc_collect,
    gc_collect_if_needed, gc_external_root_count, gc_object_count, gc_phase, gc_register_root,
    gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier, hub_create,
    hub_register, hub_unregister, hub_wait_receive, hub_wait_receive_index,
    hub_wait_receive_status, hub_wait_receive_value, mutex_create, mutex_lock, mutex_try_lock,
    mutex_unlock, panic, panic_str, str_concat, str_len, str_new, syscall_read, syscall_write,
    test_bytes_len, test_bytes_ptr, wait_group_add, wait_group_create, wait_group_done,
    wait_group_wait,
};

#[cfg(feature = "metrics")]
pub use builtins::{
    rt_metrics_alloc_bytes, rt_metrics_alloc_calls, rt_metrics_event_get_handler_calls,
    rt_metrics_event_subscribe_calls, rt_metrics_event_unsubscribe_calls,
    rt_metrics_heap_fragmentation_bytes, rt_metrics_heap_live_bytes, rt_metrics_heap_total_bytes,
    rt_metrics_str_concat_bytes, rt_metrics_str_concat_calls,
};
pub use channel::{
    channel_receive_ptr, channel_send_ptr, channel_try_receive_ptr, channel_try_send_ptr,
};
pub use gc::{
    MutatorAttachGuard, RuntimePhase, RuntimeRoot, RuntimeState, assert_mutator_allowed,
    attach_phase_b_mutator, beskid_heap_options_for_engine, clear_current_heap,
    clear_current_root, enter_runtime_scope, in_runtime_scope, is_syscall_pool_worker,
    leave_runtime_scope, preemption_enabled, runtime_phase, runtime_preempt_check,
    set_current_heap, set_current_root, set_preemption_enabled, set_runtime_phase,
    set_syscall_pool_worker, with_current_heap, with_current_root,
};
pub use interop::{interop_dispatch_ptr, interop_dispatch_unit, interop_dispatch_usize};
pub use runtime::{GcSnapshot, collect_if_needed, force_collect, snapshot_gc};
pub use scheduler::{init as scheduler_init, run_closure_as_main, run_main_fiber};
