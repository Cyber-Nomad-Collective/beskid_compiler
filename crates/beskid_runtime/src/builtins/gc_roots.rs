use crate::gc::{drop_handle, store_handle, with_current_root};

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_root_handle(value_ptr: *mut u8) -> u64 {
    with_current_root(|root| store_handle(root, value_ptr))
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_unroot_handle(handle: u64) {
    with_current_root(|root| drop_handle(root, handle));
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_register_root(ptr_addr: *mut *mut u8) {
    if ptr_addr.is_null() {
        return;
    }
    with_current_root(|root| {
        root.runtime_state.registered_roots.push(ptr_addr);
    });
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_unregister_root(ptr_addr: *mut *mut u8) {
    if ptr_addr.is_null() {
        return;
    }
    with_current_root(|root| {
        root.runtime_state
            .registered_roots
            .retain(|entry| *entry != ptr_addr);
    });
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn gc_write_barrier(_dst_obj: *mut u8, _value_ptr: *mut u8) {}
