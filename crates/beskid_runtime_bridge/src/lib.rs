//! Static library bridge exporting stable Beskid runtime symbols for AOT linking.

// Each `as usize` coercion below forces the linker to resolve the symbol.
// Suppress unused-imports at the crate level because the imports serve as
// link-time anchors only — they are not called directly from this crate.
#![allow(unused_imports)]

use beskid_runtime::{
    alloc, array_len, array_new, beskid_runtime_abi_version, channel_close, channel_create,
    channel_receive, channel_receive_status, channel_receive_value, channel_send,
    channel_try_receive, channel_try_send, event_get_handler, event_len, event_subscribe,
    event_unsubscribe_first, fiber_cancel, fiber_current_id, fiber_detach, fiber_join,
    fiber_join_status, fiber_join_value, fiber_now_millis, fiber_processor_count, fiber_spawn,
    fiber_spawn_with_cancel_slot, fiber_yield, gc_bytes_allocated, gc_collect,
    gc_collect_if_needed, gc_external_root_count, gc_object_count, gc_phase, gc_register_root,
    gc_root_handle, gc_unregister_root, gc_unroot_handle, gc_write_barrier, hub_create,
    hub_register, hub_unregister, hub_wait_receive, hub_wait_receive_index,
    hub_wait_receive_status, hub_wait_receive_value, interop_dispatch_ptr, interop_dispatch_unit,
    interop_dispatch_usize, mutex_create, mutex_lock, mutex_try_lock, mutex_unlock, panic,
    panic_str, str_concat, str_from_i64, str_len, str_new, syscall_read, syscall_write, test_bytes_len,
    test_bytes_ptr, wait_group_add, wait_group_create, wait_group_done, wait_group_wait,
};

#[unsafe(no_mangle)]
pub extern "C" fn beskid_runtime_link_anchor() {
    let _ = beskid_runtime_abi_version as usize;
    let _ = alloc as usize;
    let _ = str_new as usize;
    let _ = str_concat as usize;
    let _ = str_from_i64 as usize;
    let _ = array_new as usize;
    let _ = array_len as usize;
    let _ = panic as usize;
    let _ = panic_str as usize;
    let _ = gc_write_barrier as usize;
    let _ = gc_bytes_allocated as usize;
    let _ = gc_object_count as usize;
    let _ = gc_phase as usize;
    let _ = gc_collect as usize;
    let _ = gc_collect_if_needed as usize;
    let _ = gc_external_root_count as usize;
    let _ = gc_root_handle as usize;
    let _ = gc_unroot_handle as usize;
    let _ = gc_register_root as usize;
    let _ = gc_unregister_root as usize;
    let _ = event_subscribe as usize;
    let _ = event_unsubscribe_first as usize;
    let _ = event_len as usize;
    let _ = event_get_handler as usize;
    let _ = interop_dispatch_unit as usize;
    let _ = interop_dispatch_ptr as usize;
    let _ = interop_dispatch_usize as usize;
    let _ = syscall_write as usize;
    let _ = syscall_read as usize;
    let _ = test_bytes_ptr as usize;
    let _ = test_bytes_len as usize;
    let _ = str_len as usize;
    let _ = fiber_spawn as usize;
    let _ = fiber_spawn_with_cancel_slot as usize;
    let _ = fiber_join as usize;
    let _ = fiber_join_status as usize;
    let _ = fiber_join_value as usize;
    let _ = fiber_detach as usize;
    let _ = fiber_cancel as usize;
    let _ = fiber_yield as usize;
    let _ = fiber_now_millis as usize;
    let _ = fiber_current_id as usize;
    let _ = fiber_processor_count as usize;
    let _ = channel_create as usize;
    let _ = channel_send as usize;
    let _ = channel_receive as usize;
    let _ = channel_receive_status as usize;
    let _ = channel_receive_value as usize;
    let _ = channel_try_send as usize;
    let _ = channel_try_receive as usize;
    let _ = channel_close as usize;
    let _ = hub_create as usize;
    let _ = hub_register as usize;
    let _ = hub_unregister as usize;
    let _ = hub_wait_receive as usize;
    let _ = hub_wait_receive_status as usize;
    let _ = hub_wait_receive_index as usize;
    let _ = hub_wait_receive_value as usize;
    let _ = mutex_create as usize;
    let _ = mutex_lock as usize;
    let _ = mutex_try_lock as usize;
    let _ = mutex_unlock as usize;
    let _ = wait_group_create as usize;
    let _ = wait_group_add as usize;
    let _ = wait_group_done as usize;
    let _ = wait_group_wait as usize;
}
