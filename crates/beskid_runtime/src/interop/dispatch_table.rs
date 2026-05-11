//! Tag-driven interop handlers invoked from `interop_dispatch_*` in the parent `interop` module.

use beskid_abi::BeskidStr;

/// Interop tag for `str.len` style operations over a wrapped [`BeskidStr`].
pub const TAG_STRING_LEN: i32 = 0;

/// Returns whether `tag` was handled for unit dispatch (currently unused tags return false).
pub unsafe fn dispatch_unit(tag: i32, _enum_ptr: *const u8) -> bool {
    match tag {
        _ => false,
    }
}

/// `usize` dispatch by `tag`; [`TAG_STRING_LEN`] reads a [`BeskidStr`] pointer from the enum payload.
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
pub unsafe fn dispatch_ptr(tag: i32, _enum_ptr: *const u8) -> Option<*mut u8> {
    match tag {
        _ => None,
    }
}
