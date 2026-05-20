//! C ABI entry points invoked from generated code (`extern "C-unwind"`), backed by [`crate::gc`].
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod alloc;
mod arrays;
mod channel;
mod events;
mod fiber;
mod gc;
mod gc_roots;
mod hub;
#[cfg(feature = "metrics")]
mod metrics;
mod mutex;
mod panic_io;
#[cfg(feature = "sched")]
mod sched;
mod strings;
mod test_helpers;
mod version;
mod wait_group;

pub use alloc::alloc;
pub use arrays::{array_len, array_new};
pub use channel::{
    channel_close, channel_create, channel_receive, channel_receive_status, channel_receive_value,
    channel_send, channel_try_receive, channel_try_send,
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
pub use panic_io::{panic, panic_str, syscall_read, syscall_write};
#[cfg(feature = "sched")]
pub use sched::{rt_now_millis, rt_yield};
pub use strings::{str_concat, str_len, str_new};
pub use test_helpers::{test_bytes_len, test_bytes_ptr};
pub use version::beskid_runtime_abi_version;
pub use wait_group::{wait_group_add, wait_group_create, wait_group_done, wait_group_wait};
