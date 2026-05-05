//! Runtime support for Beskid (allocation, builtins, GC hooks).

pub mod builtins;
pub mod gc;
pub mod interop;
mod interop_layout;

pub use builtins::{
    alloc, array_new, beskid_runtime_abi_version, event_get_handler, event_len, event_subscribe,
    event_unsubscribe_first, gc_register_root, gc_root_handle, gc_unregister_root,
    gc_unroot_handle, gc_write_barrier, panic, panic_str, str_concat, str_len, str_new,
    syscall_read, syscall_write, test_bytes_len, test_bytes_ptr,
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
