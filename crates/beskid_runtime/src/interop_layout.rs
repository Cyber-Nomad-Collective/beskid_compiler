pub const ENUM_TAG_OFFSET: usize = 8;

#[inline]
pub fn enum_tag(enum_ptr: *const u8) -> i32 {
    // Interop envelopes are byte-addressed; read the tag without assuming the
    // host pointer is naturally aligned for i32 (test harnesses and foreign
    // payloads may hand back only byte alignment).
    unsafe { std::ptr::read_unaligned(enum_ptr.add(ENUM_TAG_OFFSET) as *const i32) }
}
