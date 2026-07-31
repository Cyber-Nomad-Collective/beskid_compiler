#[cfg(feature = "metrics")]
use crate::gc::with_current_root;
use beskid_abi::BeskidStr;

use super::alloc::alloc;

/// Allocate a self-contained BeskidStr and copy an existing UTF-8 byte buffer into it.
///
/// Safety/contract (v0.1):
/// - `ptr` must be non-null (even if `len` is 0) and point to at least `len` bytes.
/// - The returned header and bytes share one GC allocation, so rooting the handle retains its data.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_new(ptr: *const u8, len: usize) -> *mut BeskidStr {
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if std::str::from_utf8(bytes).is_err() {
        panic!("invalid utf-8 string data");
    }

    let header_size = std::mem::size_of::<BeskidStr>();
    let allocation = alloc(header_size.saturating_add(len), std::ptr::null());
    if allocation.is_null() {
        panic!("string allocation failed");
    }
    let target = allocation.cast::<BeskidStr>();
    let data = unsafe { allocation.add(header_size) };
    unsafe {
        std::ptr::copy_nonoverlapping(ptr, data, len);
        target.write(BeskidStr { ptr: data, len });
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

/// Concatenate two BeskidStr values into a fresh self-contained string allocation.
///
/// Safety/contract (v0.1):
/// - `left` and `right` must be non-null handles; their `.ptr` must be non-null (even if len==0).
/// - Performs byte-wise copy; inputs are assumed valid UTF-8.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_concat(left: *const BeskidStr, right: *const BeskidStr) -> *mut BeskidStr {
    if left.is_null() || right.is_null() {
        panic!("null string handle");
    }

    let (left_ptr, left_len) = unsafe { ((*left).ptr, (*left).len) };
    let (right_ptr, right_len) = unsafe { ((*right).ptr, (*right).len) };
    if left_ptr.is_null() || right_ptr.is_null() {
        panic!("null string data");
    }

    let total_len = left_len.saturating_add(right_len);
    let mut buffer = Vec::with_capacity(total_len);
    unsafe {
        buffer.extend_from_slice(std::slice::from_raw_parts(left_ptr, left_len));
        buffer.extend_from_slice(std::slice::from_raw_parts(right_ptr, right_len));
    }

    #[cfg(feature = "metrics")]
    with_current_root(|root| {
        root.runtime_state.str_concat_calls = root.runtime_state.str_concat_calls.saturating_add(1);
        root.runtime_state.str_concat_bytes = root.runtime_state.str_concat_bytes.saturating_add(total_len);
    });

    str_new(buffer.as_ptr(), total_len)
}

/// Content equality comparison for two `BeskidStr` values.
///
/// Returns 1 if both strings have the same byte length and identical content;
/// returns 0 otherwise. Null-ptr data is treated as empty content.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_eq(left: *const BeskidStr, right: *const BeskidStr) -> usize {
    if left.is_null() || right.is_null() {
        panic!("null string handle");
    }

    let (left_ptr, left_len) = unsafe { ((*left).ptr, (*left).len) };
    let (right_ptr, right_len) = unsafe { ((*right).ptr, (*right).len) };

    // Same pointer + length → trivially equal (also covers empty-empty null-ptr case).
    if left_ptr == right_ptr && left_len == right_len {
        return 1;
    }

    // Equal length → compare content byte-wise.
    if left_len == right_len {
        if left_ptr.is_null() || right_ptr.is_null() {
            // One is null-data while the other is non-null; only equal if both have len 0.
            return usize::from(left_len == 0);
        }
        let equal = unsafe {
            std::slice::from_raw_parts(left_ptr, left_len) == std::slice::from_raw_parts(right_ptr, right_len)
        };
        return usize::from(equal);
    }

    0
}

/// Format a signed integer as decimal UTF-8 in a newly allocated `BeskidStr`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_from_i64(value: i64) -> *mut BeskidStr {
    let formatted = value.to_string();
    let len = formatted.len();
    let buffer = alloc(len, std::ptr::null()).cast::<u8>();
    if buffer.is_null() {
        panic!("str_from_i64 allocation failed");
    }
    unsafe {
        std::ptr::copy_nonoverlapping(formatted.as_ptr(), buffer, len);
    }
    str_new(buffer, len)
}

/// Copy a UTF-8 substring into a newly allocated `BeskidStr`.
///
/// `start` is a byte offset; `count` is the number of bytes to copy. Out-of-range
/// inputs clamp to an empty or shorter slice rather than panicking.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_slice(value: *const BeskidStr, start: usize, count: usize) -> *mut BeskidStr {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if ptr.is_null() || len == 0 {
        static Z: [u8; 1] = [0];
        return str_new(Z.as_ptr(), 0);
    }
    let start = start.min(len);
    let end = start.saturating_add(count).min(len);
    let slice_len = end.saturating_sub(start);
    if slice_len == 0 {
        static Z: [u8; 1] = [0];
        return str_new(Z.as_ptr(), 0);
    }
    str_new(unsafe { ptr.add(start) }, slice_len)
}
