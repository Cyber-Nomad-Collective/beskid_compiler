pub const ENUM_TAG_OFFSET: usize = 8;

#[inline]
pub fn enum_tag(enum_ptr: *const u8) -> i32 {
    unsafe { *(enum_ptr.add(ENUM_TAG_OFFSET) as *const i32) }
}
