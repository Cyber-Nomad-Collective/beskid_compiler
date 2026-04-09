#[cfg(feature = "metrics")]
use crate::gc::with_current_root;
use beskid_abi::BeskidStr;

use super::alloc::alloc;

/// Allocate a BeskidStr header that points to an existing UTF-8 byte buffer.
///
/// Safety/contract (v0.1):
/// - `ptr` must be non-null (even if `len` is 0) and point to at least `len` bytes.
/// - The buffer is not copied; lifetime is managed by the caller or points to static data.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_new(ptr: *const u8, len: usize) -> *mut BeskidStr {
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if std::str::from_utf8(bytes).is_err() {
        panic!("invalid utf-8 string data");
    }

    let size = std::mem::size_of::<BeskidStr>();
    let allocation = alloc(size, std::ptr::null());
    if allocation.is_null() {
        panic!("string allocation failed");
    }
    let target = allocation.cast::<BeskidStr>();
    unsafe {
        target.write(BeskidStr { ptr, len });
    }
    target
}

/// Return string byte length; panics on null handle.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_len(value: *const BeskidStr) -> usize {
    if value.is_null() {
        panic!("null string handle");
    }
    unsafe { (*value).len }
}

/// Concatenate two BeskidStr values by allocating a fresh data buffer and header.
///
/// Safety/contract (v0.1):
/// - `left` and `right` must be non-null handles; their `.ptr` must be non-null (even if len==0).
/// - Performs byte-wise copy; inputs are assumed valid UTF-8.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_concat(
    left: *const BeskidStr,
    right: *const BeskidStr,
) -> *mut BeskidStr {
    if left.is_null() || right.is_null() {
        panic!("null string handle");
    }

    let (left_ptr, left_len) = unsafe { ((*left).ptr, (*left).len) };
    let (right_ptr, right_len) = unsafe { ((*right).ptr, (*right).len) };
    if left_ptr.is_null() || right_ptr.is_null() {
        panic!("null string data");
    }

    let total_len = left_len.saturating_add(right_len);
    let buffer = alloc(total_len, std::ptr::null()).cast::<u8>();
    if buffer.is_null() {
        panic!("string concat allocation failed");
    }

    unsafe {
        std::ptr::copy_nonoverlapping(left_ptr, buffer, left_len);
        std::ptr::copy_nonoverlapping(right_ptr, buffer.add(left_len), right_len);
    }

    #[cfg(feature = "metrics")]
    with_current_root(|root| {
        root.runtime_state.str_concat_calls = root.runtime_state.str_concat_calls.saturating_add(1);
        root.runtime_state.str_concat_bytes = root
            .runtime_state
            .str_concat_bytes
            .saturating_add(total_len);
    });

    str_new(buffer.cast::<u8>(), total_len)
}
