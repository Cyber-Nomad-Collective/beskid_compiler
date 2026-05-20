//! Tag-driven interop handlers invoked from `interop_dispatch_*` in the parent `interop` module.

use beskid_abi::BeskidStr;

/// Interop tag for `str.len` style operations over a wrapped [`BeskidStr`].
pub const TAG_STRING_LEN: i32 = 0;

/// Returns whether `tag` was handled for unit dispatch (currently unused tags return false).
///
/// # Safety
///
/// `enum_ptr` must point to a valid generated interop enum payload for the duration of dispatch.
pub unsafe fn dispatch_unit(tag: i32, _enum_ptr: *const u8) -> bool {
    let _ = tag;
    false
}

/// `usize` dispatch by `tag`; [`TAG_STRING_LEN`] reads a [`BeskidStr`] pointer from the enum payload.
///
/// # Safety
///
/// `enum_ptr` must point to a valid generated interop enum payload whose layout matches `tag`.
pub unsafe fn dispatch_usize(tag: i32, enum_ptr: *const u8) -> Option<usize> {
    match tag {
        TAG_STRING_LEN => {
            let _text = unsafe { *(enum_ptr.add(16) as *const *const BeskidStr) };
            Some(crate::builtins::str_len(_text))
        }
        _ => None,
    }
}

/// Pointer dispatch by `tag` (reserved for future interop kinds).
///
/// # Safety
///
/// `enum_ptr` must point to a valid generated interop enum payload for the duration of dispatch.
pub unsafe fn dispatch_ptr(tag: i32, _enum_ptr: *const u8) -> Option<*mut u8> {
    let _ = tag;
    None
}
