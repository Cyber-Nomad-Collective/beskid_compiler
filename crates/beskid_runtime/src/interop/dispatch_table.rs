// Runtime interop dispatch table (source-owned).

use beskid_abi::BeskidStr;

pub const TAG_STRING_LEN: i32 = 0;

pub unsafe fn dispatch_unit(tag: i32, _enum_ptr: *const u8) -> bool {
    match tag {
        _ => false,
    }
}

pub unsafe fn dispatch_usize(tag: i32, enum_ptr: *const u8) -> Option<usize> {
    match tag {
        TAG_STRING_LEN => {
            let _text = unsafe { *(enum_ptr.add(16) as *const *const BeskidStr) };
            Some(crate::builtins::str_len(_text))
        }
        _ => None,
    }
}

pub unsafe fn dispatch_ptr(tag: i32, _enum_ptr: *const u8) -> Option<*mut u8> {
    match tag {
        _ => None,
    }
}
