//! Shared Beskid string helpers for host OS builtins.

use beskid_abi::BeskidStr;

pub(crate) fn string_from_rust(text: &str) -> *mut BeskidStr {
    if text.is_empty() {
        static Z: [u8; 1] = [0];
        return beskid_runtime::builtins::str_new(Z.as_ptr(), 0);
    }
    beskid_runtime::builtins::str_new(text.as_ptr(), text.len())
}

pub(crate) fn read_string_path(value: *const BeskidStr) -> String {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if len == 0 {
        return String::new();
    }
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes)
        .unwrap_or_else(|_| panic!("invalid utf-8 path"))
        .to_string()
}
