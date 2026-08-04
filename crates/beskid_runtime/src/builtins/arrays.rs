use abfall::ArrayElementDescriptor;
use beskid_abi::BeskidArray;

use crate::gc::with_current_heap_and_root;

use super::alloc::alloc;

/// ABI-v5 request whose field order is frozen by `runtime_manifest.bsol`.
#[repr(C)]
pub struct ArrayAllocationRequest {
    pub element: *const ArrayElementDescriptor,
    pub length: usize,
    pub flags: u32,
    pub reserved: u32,
}

const ARRAY_FLAG_NONE: u32 = 0;

fn valid_element_descriptor(descriptor: &ArrayElementDescriptor) -> bool {
    if descriptor.stride == 0 || !descriptor.alignment.is_power_of_two() || descriptor.alignment == 0 {
        return false;
    }
    if descriptor.stride % descriptor.alignment != 0 {
        return false;
    }
    if descriptor.pointer_count == 0 {
        return true;
    }
    if descriptor.pointer_map.is_null() {
        return false;
    }
    let pointer_size = std::mem::size_of::<*mut u8>();
    let mut previous = None;
    for index in 0..descriptor.pointer_count {
        // SAFETY: ABI callers must pass an immutable codegen-emitted descriptor. Validation
        // below rejects all malformed offsets before the metadata is published to the heap.
        let offset = unsafe { std::ptr::read_unaligned(descriptor.pointer_map.add(index)) };
        let Some(end) = offset.checked_add(pointer_size) else {
            return false;
        };
        if offset % pointer_size != 0 || end > descriptor.stride {
            return false;
        }
        if previous.is_some_and(|previous| previous >= offset) {
            return false;
        }
        previous = Some(offset);
    }
    true
}

fn allocate_typed_array(
    request: *const ArrayAllocationRequest,
    root_handle_out: Option<*mut usize>,
) -> *mut BeskidArray {
    if request.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: C ABI request layout is manifest-frozen; all pointed-to metadata is validated
    // before it is copied into a GC object.
    let request = unsafe { &*request };
    if request.flags != ARRAY_FLAG_NONE || request.reserved != 0 || request.element.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `element` is a caller-supplied ABI pointer. `valid_element_descriptor` rejects
    // malformed layouts before allocation/publishing; codegen supplies static data only.
    let descriptor = unsafe { *request.element };
    if !valid_element_descriptor(&descriptor) {
        return std::ptr::null_mut();
    }
    let header_size = std::mem::size_of::<BeskidArray>();
    if descriptor.stride.checked_mul(request.length).and_then(|bytes| header_size.checked_add(bytes)).is_none() {
        return std::ptr::null_mut();
    }
    with_current_heap_and_root(|heap, root| {
        let payload = heap.allocate_beskid_array(header_size, descriptor, request.length);
        if payload.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: `allocate_beskid_array` reserved exactly header_size + backing bytes.
        let data = unsafe { payload.add(header_size) };
        let array = payload.cast::<BeskidArray>();
        // SAFETY: header is uniquely owned until returned to generated code.
        unsafe { array.write(BeskidArray { ptr: data, len: request.length, cap: request.length }) };
        if let Some(root_handle_out) = root_handle_out {
            let handle = root.heap.external_roots().push_handle(array.cast());
            // SAFETY: the rooted ABI entrypoint requires a writable native-word stack slot.
            unsafe { root_handle_out.write(handle as usize) };
        }
        root.runtime_state.allocation_counter = root.runtime_state.allocation_counter.saturating_add(1);
        let live_bytes = heap.bytes_allocated();
        root.runtime_state.heap_total_bytes = root.runtime_state.heap_total_bytes.saturating_add(header_size).max(live_bytes);
        root.runtime_state.heap_live_bytes = live_bytes;
        array
    })
}

/// Allocate a descriptor-backed managed array through the ABI-v5 native runtime.
///
/// This is intentionally separate from the historical `array_new(element_size, len)` surface:
/// generic byte allocation has no pointer-map authority and therefore cannot represent managed
/// reference elements. Invalid requests return null and no descriptor is synthesized from size.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_rt_v5_array_allocate(request: *const ArrayAllocationRequest) -> *mut BeskidArray {
    allocate_typed_array(request, None)
}

/// Allocate an array and publish a temporary root before generated code can lower any element.
///
/// `root_handle_out` is an ABI-owned stack slot. The caller must pass its token to
/// [`beskid_rt_v5_array_construction_finish`] after every element store/barrier is complete.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_rt_v5_array_allocate_rooted(
    request: *const ArrayAllocationRequest,
    root_handle_out: *mut usize,
) -> *mut BeskidArray {
    if root_handle_out.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: caller supplied a valid native-word output slot under the ABI contract.
    unsafe { root_handle_out.write(usize::MAX) };
    allocate_typed_array(request, Some(root_handle_out))
}

/// Release the temporary root established by the rooted array allocator.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_rt_v5_array_construction_finish(root_handle: *mut u8) -> u8 {
    let root_handle = root_handle as usize;
    if root_handle == usize::MAX {
        return 0;
    }
    with_current_heap_and_root(|_heap, root| {
        root.heap.external_roots().drop_handle(root_handle as u64);
        1
    })
}

/// Publish a pointer-element write performed by generated typed-array code.
///
/// Generated code stores the value at a descriptor-authorized offset, then calls this helper.
/// The helper rejects foreign/collected destinations and applies the heap's Dijkstra insertion
/// barrier while marking. It deliberately has no scalar-array path.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn beskid_rt_v5_array_write_barrier(array: *mut BeskidArray, value: *mut u8) -> u8 {
    if array.is_null() {
        return 0;
    }
    with_current_heap_and_root(|heap, _root| {
        if !heap.owns_beskid_payload(array.cast()) {
            return 0;
        }
        heap.write_barrier(array.cast(), value);
        1
    })
}

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
