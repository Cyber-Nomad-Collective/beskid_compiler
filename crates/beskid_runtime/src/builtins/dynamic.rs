//! C ABI builtins for dynamic cells and object-to-object mapping.

use crate::builtins::alloc;
use crate::dynamic::{
    DynamicCell, DYNAMIC_ERR_INCOMPATIBLE, DYNAMIC_ERR_NULL_PAYLOAD, DYNAMIC_OK,
    map_dynamic_fallback, map_objects_aot,
};
use crate::gc::with_current_heap_and_root;

const ABI_OK: i32 = DYNAMIC_OK;

/// Allocate a dynamic cell header; `payload` may be null until bound.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn dynamic_cell_create(shape_id: u32, payload: *mut u8) -> *mut DynamicCell {
    with_current_heap_and_root(|heap, root| {
        let ptr = heap.allocate_beskid(DynamicCell::SIZE, std::ptr::null());
        root.runtime_state.allocation_counter += 1;
        // Safety: fresh allocation sized for `DynamicCell`.
        let cell = unsafe { &mut *(ptr as *mut DynamicCell) };
        *cell = DynamicCell {
            shape_id,
            flags: 0,
            payload,
        };
        ptr as *mut DynamicCell
    })
}

/// Wrap an existing static object pointer in a dynamic cell (no payload copy).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn dynamic_cell_wrap(shape_id: u32, static_ptr: *mut u8) -> *mut DynamicCell {
    dynamic_cell_create(shape_id, static_ptr)
}

/// Checked cast: returns `ABI_OK` when `cell.shape_id == expected_shape`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn dynamic_cast_checked(cell: *mut DynamicCell, expected_shape: u32) -> i32 {
    if cell.is_null() {
        return DYNAMIC_ERR_NULL_PAYLOAD;
    }
    // Safety: caller owns a cell pointer from `dynamic_cell_create` / `dynamic_cell_wrap`.
    let cell = unsafe { &*cell };
    if cell.shape_id == expected_shape {
        ABI_OK
    } else {
        DYNAMIC_ERR_INCOMPATIBLE
    }
}

/// AOT object-to-object mapping between registered shapes.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn dynamic_map_aot(
    src_shape: u32,
    dst_shape: u32,
    src_ptr: *const u8,
    dst_out: *mut u8,
) -> i32 {
    unsafe { map_objects_aot(src_shape, dst_shape, src_ptr, dst_out) }
}

/// Runtime fallback mapping from a dynamic cell to `dst_shape`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn dynamic_map_fallback(
    cell: *mut DynamicCell,
    dst_shape: u32,
    dst_out: *mut u8,
) -> i32 {
    if cell.is_null() {
        return DYNAMIC_ERR_NULL_PAYLOAD;
    }
    let cell = unsafe { &*cell };
    unsafe { map_dynamic_fallback(cell, dst_shape, dst_out) }
}

/// Allocate a zeroed object of `size` bytes through the runtime arena (mapping targets).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn dynamic_object_alloc(size: usize) -> *mut u8 {
    alloc(size, std::ptr::null())
}
