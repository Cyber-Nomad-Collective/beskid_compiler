//! C-compatible views of Beskid heap objects passed across the JIT boundary.

#[repr(C)]
#[derive(Debug, Clone, Copy)]
/// UTF-8 string view: `ptr`/`len` refer to bytes owned elsewhere or in static storage.
pub struct BeskidStr {
    pub ptr: *const u8,
    pub len: usize,
}

/// Growable array header; element storage depends on runtime `arrays_backing` feature.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeskidArray {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}
