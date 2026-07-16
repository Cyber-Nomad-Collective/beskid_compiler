//! Beskid-specific opaque heap objects and descriptor layout.

use crate::Heap;
use crate::trace::{Trace, Tracer};

/// Descriptor blob emitted by `beskid_codegen::module_emission::build_descriptor_data`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TypeDescriptor {
    pub size: usize,
    pub align: usize,
    pub pointer_count: u32,
    pub pointer_offsets: *const usize,
    pub name: *const u8,
}

impl TypeDescriptor {
    /// Return the descriptor's exact pointer map.
    ///
    /// The descriptor and offset table must be static codegen data and must remain valid for the
    /// returned lifetime. This is the only layout authority used to scan opaque heap payloads.
    #[inline]
    pub unsafe fn pointer_map(&self) -> &[usize] {
        if self.pointer_count == 0 || self.pointer_offsets.is_null() {
            return &[];
        }
        // SAFETY: upheld by the caller; see this method's contract.
        unsafe { std::slice::from_raw_parts(self.pointer_offsets, self.pointer_count as usize) }
    }
}

/// Opaque payload used by Beskid runtime `alloc(size, type_desc_ptr)`.
pub struct BeskidObject {
    pub(crate) heap: *const Heap,
    pub(crate) type_desc: *const TypeDescriptor,
    pub(crate) bytes: Box<[u8]>,
}

unsafe impl Trace for BeskidObject {
    fn trace(&self, tracer: &Tracer) {
        if self.type_desc.is_null() {
            return;
        }
        // SAFETY: `type_desc` points to static descriptor data emitted by codegen.
        let descriptor = unsafe { *self.type_desc };
        // SAFETY: `type_desc` is codegen-emitted static descriptor data, installed at allocation.
        for offset in unsafe { descriptor.pointer_map() } {
            if *offset + std::mem::size_of::<*mut u8>() > self.bytes.len() {
                continue;
            }
            // SAFETY: bounds-checked above; load unaligned pointer-sized field.
            let value_ptr = unsafe {
                std::ptr::read_unaligned(self.bytes.as_ptr().add(*offset).cast::<*mut u8>())
            };
            if value_ptr.is_null() || self.heap.is_null() {
                continue;
            }
            // SAFETY: heap pointer is injected from `Heap::allocate_beskid` and
            // outlives managed objects in that heap.
            unsafe { &*self.heap }.mark_payload_ptr(value_ptr, tracer);
        }
    }
}
