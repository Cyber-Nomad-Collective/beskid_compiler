//! Static library bridge exporting stable Beskid runtime symbols for AOT linking.

#[allow(unused_imports)]
use beskid_runtime::{
    alloc, array_new, beskid_runtime_abi_version, channel_close, channel_create, channel_receive,
    channel_send, channel_try_receive, channel_try_send, event_get_handler, event_len,
    event_subscribe, event_unsubscribe_first, fiber_cancel, fiber_current_id, fiber_detach,
    fiber_join, fiber_now_millis, fiber_spawn, fiber_yield, gc_register_root, gc_root_handle,
    gc_unregister_root, gc_unroot_handle, gc_write_barrier, hub_create, hub_register,
    hub_unregister, hub_wait_receive, interop_dispatch_ptr, interop_dispatch_unit,
    interop_dispatch_usize, mutex_create, mutex_lock, mutex_try_lock, mutex_unlock, panic,
    panic_str, str_concat, str_len, str_new, syscall_read, syscall_write, test_bytes_len,
    test_bytes_ptr, wait_group_add, wait_group_create, wait_group_done, wait_group_wait,
};

#[unsafe(no_mangle)]
pub extern "C" fn beskid_runtime_link_anchor() {
    let _ = beskid_runtime_abi_version as usize;
    let _ = alloc as usize;
    let _ = str_new as usize;
    let _ = str_concat as usize;
    let _ = array_new as usize;
    let _ = panic as usize;
    let _ = panic_str as usize;
    let _ = gc_write_barrier as usize;
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
    let _ = fiber_join as usize;
    let _ = fiber_detach as usize;
    let _ = fiber_cancel as usize;
    let _ = fiber_yield as usize;
    let _ = fiber_now_millis as usize;
    let _ = fiber_current_id as usize;
    let _ = channel_create as usize;
    let _ = channel_send as usize;
    let _ = channel_receive as usize;
    let _ = channel_try_send as usize;
    let _ = channel_try_receive as usize;
    let _ = channel_close as usize;
    let _ = hub_create as usize;
    let _ = hub_register as usize;
    let _ = hub_unregister as usize;
    let _ = hub_wait_receive as usize;
    let _ = mutex_create as usize;
    let _ = mutex_lock as usize;
    let _ = mutex_try_lock as usize;
    let _ = mutex_unlock as usize;
    let _ = wait_group_create as usize;
    let _ = wait_group_add as usize;
    let _ = wait_group_done as usize;
    let _ = wait_group_wait as usize;
}
