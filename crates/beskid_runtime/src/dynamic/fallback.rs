//! Runtime fallback mapper when static AOT shape information is unavailable at compile time.

use super::cell::DynamicCell;
use super::table::{mapping_steps, shape_object_size, FieldStep};

/// Stable ABI status codes surfaced to generated code and tests.
pub const DYNAMIC_OK: i32 = 0;
/// Deterministic incompatibility (`E-dynamic-map-001` in platform spec).
pub const DYNAMIC_ERR_INCOMPATIBLE: i32 = 1;
pub const DYNAMIC_ERR_UNKNOWN_SRC_SHAPE: i32 = 2;
pub const DYNAMIC_ERR_UNKNOWN_DST_SHAPE: i32 = 3;
pub const DYNAMIC_ERR_NULL_PAYLOAD: i32 = 4;

fn copy_fields(src: *const u8, dst: *mut u8, steps: &[FieldStep]) {
    for step in steps {
        // Safety: callers guarantee offsets/sizes lie within registered object sizes.
        unsafe {
            let from = src.add(step.src_offset as usize);
            let to = dst.add(step.dst_offset as usize);
            core::ptr::copy_nonoverlapping(from, to, step.size as usize);
        }
    }
}

/// Map a [`DynamicCell`] payload to `dst_shape`, writing into `dst_out` (caller-sized buffer).
///
/// # Safety
///
/// `dst_out` must point to at least `shape_object_size(dst_shape)` writable bytes.
pub unsafe fn map_dynamic_fallback(cell: &DynamicCell, dst_shape: u32, dst_out: *mut u8) -> i32 {
    if cell.payload.is_null() {
        return DYNAMIC_ERR_NULL_PAYLOAD;
    }
    if shape_object_size(cell.shape_id).is_none() {
        return DYNAMIC_ERR_UNKNOWN_SRC_SHAPE;
    }
    let Some(dst_size) = shape_object_size(dst_shape) else {
        return DYNAMIC_ERR_UNKNOWN_DST_SHAPE;
    };
    let Some(steps) = mapping_steps(cell.shape_id, dst_shape) else {
        return DYNAMIC_ERR_INCOMPATIBLE;
    };
    if dst_out.is_null() {
        return DYNAMIC_ERR_INCOMPATIBLE;
    }
    // Zero the destination object before applying the registered field steps.
    unsafe {
        core::ptr::write_bytes(dst_out, 0, dst_size);
    }
    copy_fields(cell.payload, dst_out, &steps);
    DYNAMIC_OK
}

/// AOT/static path: map between two known shapes using the same table (no `DynamicCell` wrapper).
///
/// # Safety
///
/// `src_ptr` and `dst_out` must point to valid buffers of the registered shape sizes.
pub unsafe fn map_objects_aot(
    src_shape: u32,
    dst_shape: u32,
    src_ptr: *const u8,
    dst_out: *mut u8,
) -> i32 {
    if src_ptr.is_null() || dst_out.is_null() {
        return DYNAMIC_ERR_INCOMPATIBLE;
    }
    if shape_object_size(src_shape).is_none() {
        return DYNAMIC_ERR_UNKNOWN_SRC_SHAPE;
    }
    let Some(dst_size) = shape_object_size(dst_shape) else {
        return DYNAMIC_ERR_UNKNOWN_DST_SHAPE;
    };
    let Some(steps) = mapping_steps(src_shape, dst_shape) else {
        return DYNAMIC_ERR_INCOMPATIBLE;
    };
    unsafe {
        core::ptr::write_bytes(dst_out, 0, dst_size);
    }
    copy_fields(src_ptr, dst_out, &steps);
    DYNAMIC_OK
}
