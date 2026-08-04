//! Beskid-specific opaque heap objects and descriptor layout.

use crate::Heap;
use crate::trace::{Trace, Tracer};

/// Zeroed byte storage with native-word alignment.
///
/// ABI-v5 array headers are cast to `BeskidArray` by the native runtime. A `Box<[u8]>` has only
/// byte alignment, whereas this storage is aligned at least as strongly as every supported
/// `BeskidArray` field. The length remains byte-exact and is never rounded for tracing bounds.
pub(crate) struct AlignedBytes {
    words: Box<[usize]>,
    byte_len: usize,
}

impl AlignedBytes {
    pub(crate) fn zeroed(byte_len: usize) -> Self {
        let word = std::mem::size_of::<usize>();
        let words = byte_len.saturating_add(word - 1) / word;
        Self { words: vec![0usize; words].into_boxed_slice(), byte_len }
    }

    pub(crate) fn len(&self) -> usize {
        self.byte_len
    }

    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.words.as_ptr().cast()
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut u8 {
        self.words.as_mut_ptr().cast()
    }
}

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

/// Immutable ABI-v5 element metadata for one managed array backing store.
///
/// Unlike [`TypeDescriptor`], offsets are relative to each element (not an object header).
/// The descriptor is codegen-owned static data and is validated once before the heap publishes
/// the allocation.  Keeping a copied descriptor in [`BeskidObject`] prevents an array from
/// depending on a mutator-owned request record after allocation returns.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ArrayElementDescriptor {
    pub stride: usize,
    pub alignment: usize,
    pub pointer_map: *const usize,
    pub pointer_count: usize,
}

/// Persistent tracing metadata for an in-buffer typed array.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BeskidArrayMetadata {
    pub descriptor: ArrayElementDescriptor,
    pub length: usize,
    pub data_offset: usize,
}

impl TypeDescriptor {
    /// Return the descriptor's exact pointer map.
    ///
    /// The descriptor and offset table must be static codegen data and must remain valid for the
    /// returned lifetime. This is the only layout authority used to scan opaque heap payloads.
    ///
    /// # Safety
    ///
    /// `pointer_offsets` must either be null when `pointer_count` is zero, or point to at least
    /// `pointer_count` valid `usize` entries for the returned slice lifetime.
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
    pub(crate) bytes: AlignedBytes,
    /// Present only for ABI-v5 typed arrays.  This is intentionally stored with the allocation,
    /// rather than reconstructed from an array header or generic element size during tracing.
    pub(crate) array: Option<BeskidArrayMetadata>,
}

unsafe impl Trace for BeskidObject {
    fn trace(&self, tracer: &Tracer) {
        if !self.type_desc.is_null() {
            // SAFETY: `type_desc` points to static descriptor data emitted by codegen.
            let descriptor = unsafe { *self.type_desc };
            // SAFETY: `type_desc` is codegen-emitted static descriptor data, installed at allocation.
            for offset in unsafe { descriptor.pointer_map() } {
                if *offset + std::mem::size_of::<*mut u8>() > self.bytes.len() {
                    continue;
                }
                // SAFETY: bounds-checked above; load unaligned pointer-sized field.
                let value_ptr = unsafe { std::ptr::read_unaligned(self.bytes.as_ptr().add(*offset).cast::<*mut u8>()) };
                if value_ptr.is_null() || self.heap.is_null() {
                    continue;
                }
                // SAFETY: heap pointer is injected from `Heap::allocate_beskid` and
                // outlives managed objects in that heap.
                unsafe { &*self.heap }.mark_payload_ptr(value_ptr, tracer);
            }
        }

        let Some(array) = self.array else {
            return;
        };
        let descriptor = array.descriptor;
        if descriptor.pointer_count == 0 || descriptor.pointer_map.is_null() {
            return;
        }
        // SAFETY: allocation rejects malformed descriptors before publishing the object and the
        // compiler owns the static descriptor/map for the module lifetime.
        let offsets = unsafe { std::slice::from_raw_parts(descriptor.pointer_map, descriptor.pointer_count) };
        for index in 0..array.length {
            let Some(element_base) = index
                .checked_mul(descriptor.stride)
                .and_then(|offset| array.data_offset.checked_add(offset))
            else {
                return;
            };
            for offset in offsets {
                let Some(slot) = element_base.checked_add(*offset) else {
                    return;
                };
                let Some(end) = slot.checked_add(std::mem::size_of::<*mut u8>()) else {
                    return;
                };
                if end > self.bytes.len() {
                    return;
                }
                // SAFETY: the checked slot lies within the byte-owned allocation.  Pointer
                // fields may be unaligned in raw ABI storage, so use an unaligned load.
                let value_ptr = unsafe { std::ptr::read_unaligned(self.bytes.as_ptr().add(slot).cast::<*mut u8>()) };
                if !value_ptr.is_null() && !self.heap.is_null() {
                    // SAFETY: heap is installed by Heap allocation and outlives its objects.
                    unsafe { &*self.heap }.mark_payload_ptr(value_ptr, tracer);
                }
            }
        }
    }
}
