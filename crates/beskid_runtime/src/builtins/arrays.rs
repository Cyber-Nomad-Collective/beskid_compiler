use beskid_abi::BeskidArray;

use super::alloc::alloc;
use crate::gc::with_current_heap;

/// Allocate a [`BeskidArray`] header with zero-filled element backing storage.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn array_new(elem_size: usize, len: usize) -> *mut BeskidArray {
    let size = std::mem::size_of::<BeskidArray>();
    let allocation = alloc(size, std::ptr::null());
    if allocation.is_null() {
        panic!("array allocation failed");
    }
    let target = allocation.cast::<BeskidArray>();

    let data_ptr = {
        let bytes = elem_size.saturating_mul(len);
        if bytes == 0 {
            // Non-null sentinel for zero-length arrays (same pattern as str_slice).
            static Z: [u8; 1] = [0];
            Z.as_ptr() as *mut u8
        } else {
            let ptr = alloc(bytes, std::ptr::null());
            if ptr.is_null() {
                panic!("array backing allocation failed");
            }
            ptr // alloc zero-fills
        }
    };

    unsafe {
        target.write(BeskidArray { ptr: data_ptr, len, cap: len });
    }
    if len != 0 {
        with_current_heap(|heap| heap.publish_composite_beskid_edge(target.cast(), data_ptr));
    }
    target
}

/// Return logical element count for a [`BeskidArray`] handle. Null yields `0`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn array_len(value: *const BeskidArray) -> usize {
    if value.is_null() {
        return 0;
    }
    unsafe { (*value).len }
}
