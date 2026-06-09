//! GC object layout and metadata
//!
//! This module defines the internal structure of garbage-collected objects,
//! including the header, vtable, and container.

use crate::color::Color;
use crate::trace::{Trace, Tracer};
use std::alloc::Layout;
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

// Bit layout for GcHeader::header_word (single AtomicUsize):
//   bits 0-1  : tri-color mark  (White=0, Gray=1, Black=2)
//   bits 2+   : root count       (one unit = 1 << 2)
const COLOR_MASK: usize = 0b11;
const ROOT_COUNT_SHIFT: usize = 2;
const ROOT_COUNT_ONE: usize = 1 << ROOT_COUNT_SHIFT;

const WHITE_BITS: usize = Color::White as usize;
const GRAY_BITS: usize = Color::Gray as usize;
const BLACK_BITS: usize = Color::Black as usize;

/// Type-erased virtual table for GC operations
///
/// This vtable contains all type-specific operations needed for GC,
/// stored statically to avoid per-object overhead.
pub struct GcVTable {
    /// Trace function for marking reachable objects
    pub trace: unsafe fn(*const GcHeader, &Tracer),

    /// Drop function - properly drops the object using Box::from_raw
    pub drop: unsafe fn(*mut GcHeader),

    /// Layout of the complete GcBox<T>
    pub layout: Layout,
}

impl GcVTable {
    /// Create a new vtable for type T
    const fn new<T: Trace>() -> Self {
        // Compile-time assertion: header must be at offset 0 due to repr(C)
        const _: () = assert!(std::mem::offset_of!(GcBox<()>, header) == 0);

        unsafe fn trace_noop(_ptr: *const GcHeader, _tracer: &Tracer) {
            // No-op trace for types that have NO_TRACE=true
        }

        unsafe fn trace_impl<T: Trace>(ptr: *const GcHeader, tracer: &Tracer) {
            unsafe {
                // Calculate GcBox pointer from header pointer using offset
                // SAFETY: GcBox is repr(C) so header is at offset 0
                let gc_box_ptr = (ptr as *const u8).sub(std::mem::offset_of!(GcBox<T>, header))
                    as *const GcBox<T>;

                let data = &(*gc_box_ptr).data;
                data.trace(tracer);
            }
        }

        unsafe fn drop_impl<T>(ptr: *mut GcHeader) {
            unsafe {
                // Calculate GcBox pointer from header pointer using offset
                // SAFETY: GcBox is repr(C) so header is at offset 0
                let gc_box_ptr =
                    (ptr as *mut u8).sub(std::mem::offset_of!(GcBox<T>, header)) as *mut GcBox<T>;

                let _box = Box::from_raw(gc_box_ptr);
                // Box drops T here
            }
        }

        Self {
            trace: if T::NO_TRACE {
                trace_noop
            } else {
                trace_impl::<T>
            },
            drop: drop_impl::<T>,
            layout: Layout::new::<GcBox<T>>(),
        }
    }
}

/// Type-erased header for all GC objects
///
/// This header is shared by all `GcBox<T>` instances and allows
/// uniform handling of objects in the allocation list.
///
/// Color and root-count are packed into a single `AtomicUsize`:
/// bits 0-1 carry the tri-color state, bits 2+ carry the root count.
/// This saves one word (8 bytes on 64-bit) versus separate atomics.
pub struct GcHeader {
    header_word: AtomicUsize,
    /// Next pointer in the intrusive linked list
    pub next: AtomicPtr<GcHeader>,
    /// Static vtable reference for type-erased operations
    pub vtable: &'static GcVTable,
}

impl GcHeader {
    #[inline]
    fn new(vtable: &'static GcVTable, rooted: bool) -> Self {
        let word = if rooted { ROOT_COUNT_ONE } else { 0 };
        Self {
            header_word: AtomicUsize::new(word),
            next: AtomicPtr::new(null_mut()),
            vtable,
        }
    }

    pub fn inc_root(&self) {
        self.header_word
            .fetch_add(ROOT_COUNT_ONE, Ordering::Relaxed);
    }

    pub fn dec_root(&self) {
        self.header_word
            .fetch_sub(ROOT_COUNT_ONE, Ordering::Relaxed);
    }

    pub fn is_root(&self) -> bool {
        self.header_word.load(Ordering::Relaxed) >> ROOT_COUNT_SHIFT > 0
    }

    pub fn is_white(&self) -> bool {
        let word = self.header_word.load(Ordering::Acquire);
        (word & COLOR_MASK) == WHITE_BITS && (word >> ROOT_COUNT_SHIFT) == 0
    }

    pub fn mark_white_to_gray(&self) -> bool {
        let mut current = self.header_word.load(Ordering::Acquire);
        loop {
            if (current & COLOR_MASK) != WHITE_BITS {
                return false;
            }
            let new = (current & !COLOR_MASK) | GRAY_BITS;
            match self.header_word.compare_exchange_weak(
                current,
                new,
                Ordering::SeqCst,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn mark_black(&self) {
        let mut current = self.header_word.load(Ordering::Acquire);
        loop {
            let new = (current & !COLOR_MASK) | BLACK_BITS;
            match self.header_word.compare_exchange_weak(
                current,
                new,
                Ordering::Release,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn reset_white(&self) {
        self.header_word.fetch_and(!COLOR_MASK, Ordering::Release);
    }

}

/// A garbage collected object with metadata
///
/// `GcBox` wraps a value with GC metadata including color and root status.
///
/// SAFETY: repr(C) ensures that `header` is always at offset 0, making it
/// safe to cast between `*GcHeader` and `*GcBox<T>`.
#[repr(C)]
pub struct GcBox<T: ?Sized> {
    pub header: GcHeader,
    pub data: T,
}

impl<T: Trace> GcBox<T> {
    const VTABLE: GcVTable = GcVTable::new::<T>();

    /// Allocate a new GcBox using Box (idiomatic Rust!)
    pub(crate) fn new(data: T) -> NonNull<GcBox<T>> {
        Self::new_with_root(data, true)
    }

    /// Allocate a new `GcBox` and choose initial rooted state.
    pub(crate) fn new_with_root(data: T, rooted: bool) -> NonNull<GcBox<T>> {
        let gc_box = Box::new(GcBox {
            header: GcHeader::new(&Self::VTABLE, rooted),
            data,
        });

        // Leak the box to get a raw pointer
        NonNull::from(Box::leak(gc_box))
    }
}
