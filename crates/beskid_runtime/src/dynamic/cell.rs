//! In-heap [`DynamicCell`] header for v0.3 dynamic typing (Phase A single mutator).

/// GC-visible dynamic value cell: shape tag + payload pointer.
///
/// Payload objects are allocated separately through the runtime arena (`alloc`); the cell
/// header itself is also arena-allocated. Payload pointers are traced when non-null.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DynamicCell {
    pub shape_id: u32,
    pub flags: u32,
    pub payload: *mut u8,
}

impl DynamicCell {
    pub const SIZE: usize = core::mem::size_of::<Self>();

    #[must_use]
    pub const fn empty() -> Self {
        Self { shape_id: 0, flags: 0, payload: core::ptr::null_mut() }
    }
}
