//! Runtime support for Beskid: GC-arena allocation, cooperative fibers, channels, and JIT TLS hooks.
//!
//! Symbols and layouts are shared with [`beskid_abi`] for codegen and linker registration.
//!
//! Phase A GC: only one OS thread runs [`gc_arena::Mutation`] at a time; syscall pool workers
//! must not allocate from the arena without holding the scheduler mutator rules.

pub mod builtins;
pub mod channel;
pub mod fiber;
pub mod gc;
pub mod hub;
pub mod interop;
mod interop_layout;
pub mod mutex;
pub mod scheduler;
pub mod status;
pub mod wait_group;

pub use builtins::{
    alloc, array_len, array_new, beskid_runtime_abi_version, channel_close, channel_create,
    channel_receive, channel_receive_status, channel_receive_value, channel_send,
    channel_try_receive, channel_try_send, event_get_handler, event_len, event_subscribe,
    event_unsubscribe_first, fiber_cancel, fiber_current_id, fiber_detach, fiber_join,
    fiber_join_status, fiber_join_value, fiber_now_millis, fiber_spawn, fiber_spawn_with_cancel_slot,
    fiber_yield, gc_register_root, gc_root_handle, gc_unregister_root, gc_unroot_handle,
    gc_write_barrier, hub_create, hub_register, hub_unregister, hub_wait_receive,
    hub_wait_receive_index, hub_wait_receive_status, hub_wait_receive_value, mutex_create,
    mutex_lock,
    mutex_try_lock, mutex_unlock, panic, panic_str, str_concat, str_len, str_new, syscall_read,
    syscall_write, test_bytes_len, test_bytes_ptr, wait_group_add, wait_group_create,
    wait_group_done, wait_group_wait,
};

#[cfg(feature = "sched")]
pub use builtins::{rt_now_millis, rt_yield};

#[cfg(feature = "metrics")]
pub use builtins::{
    rt_metrics_alloc_bytes, rt_metrics_alloc_calls, rt_metrics_event_get_handler_calls,
    rt_metrics_event_subscribe_calls, rt_metrics_event_unsubscribe_calls,
    rt_metrics_heap_fragmentation_bytes, rt_metrics_heap_live_bytes, rt_metrics_heap_total_bytes,
    rt_metrics_str_concat_bytes, rt_metrics_str_concat_calls,
};
pub use gc::{
    RawAllocation, RuntimeRoot, RuntimeState, clear_current_mutation, clear_current_root,
    enter_runtime_scope, leave_runtime_scope, set_current_mutation, set_current_root,
    with_current_mutation, with_current_root,
};
pub use interop::{interop_dispatch_ptr, interop_dispatch_unit, interop_dispatch_usize};
pub use scheduler::{init as scheduler_init, run_closure_as_main, run_main_fiber};
