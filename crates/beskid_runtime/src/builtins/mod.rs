//! C ABI entry points invoked from generated code (`extern "C-unwind"`), backed by [`crate::gc`].
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod alloc;
mod arrays;
mod bytes;
pub mod callback;
mod channel;
mod clocks;
mod composition;
mod dynamic;
mod events;
mod fiber;
mod gc;
mod gc_roots;
mod hub;
#[cfg(feature = "metrics")]
mod metrics;
mod mutex;
mod panic_io;
mod strings;
mod test_helpers;
mod version;
mod wait_group;

pub use crate::interop::register::{
    HandlerTableEntry, beskid_register_handlers,
};
pub use alloc::alloc;
pub use arrays::{array_len, array_new};
pub use bytes::{
    bytes_compare, bytes_copy, bytes_from_str, bytes_get, bytes_set, str_from_bytes_utf8,
};
pub use callback::{
    CallbackTableEntry, beskid_register_callbacks, install_callback_trampoline,
    registered_callbacks,
};
pub use channel::{
    channel_close, channel_create, channel_receive, channel_receive_ptr, channel_receive_status,
    channel_receive_value, channel_send, channel_send_ptr, channel_try_receive,
    channel_try_receive_ptr, channel_try_send, channel_try_send_ptr,
};
pub use clocks::{clock_monotonic_nanos, clock_realtime_nanos};
pub use composition::{
    composition_bind_plural, composition_container_create, composition_container_drop,
    composition_launch, composition_register, composition_resolve, composition_resolve_plural,
    composition_scope_depth, composition_scope_enter, composition_scope_leave,
    composition_shutdown,
};
pub use dynamic::{
    dynamic_cast_checked, dynamic_cell_create, dynamic_cell_wrap, dynamic_map_aot,
    dynamic_map_fallback, dynamic_object_alloc,
};
pub use events::{
    EventState, event_get_handler, event_len, event_subscribe, event_unsubscribe_first,
};
pub use fiber::{
    fiber_cancel, fiber_current_id, fiber_detach, fiber_join, fiber_join_status, fiber_join_value,
    fiber_now_millis, fiber_processor_count, fiber_spawn, fiber_spawn_with_cancel_slot,
    fiber_yield,
};
pub use gc::{
    gc_bytes_allocated, gc_collect, gc_collect_if_needed, gc_external_root_count, gc_object_count,
    gc_phase,
};
pub use gc_roots::{
    gc_register_root, gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier,
};
pub use hub::{
    hub_create, hub_register, hub_unregister, hub_wait_receive, hub_wait_receive_index,
    hub_wait_receive_status, hub_wait_receive_value,
};
#[cfg(feature = "metrics")]
pub use metrics::*;
pub use mutex::{mutex_create, mutex_lock, mutex_try_lock, mutex_unlock};
pub use panic_io::{
    panic, panic_str, syscall_read, syscall_read_bytes, syscall_write, syscall_write_bytes,
};
pub use strings::{str_concat, str_eq, str_from_i64, str_len, str_new, str_slice};
pub use test_helpers::{test_bytes_len, test_bytes_ptr};
pub use version::beskid_runtime_abi_version;
pub use wait_group::{wait_group_add, wait_group_create, wait_group_done, wait_group_wait};
