use beskid_abi::{BeskidArray, BeskidStr};

use super::arrays::array_new;
use super::strings::str_new;

/// Copy UTF-8 octets from a string into a new `u8[]` (element size 1).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bytes_from_str(value: *const BeskidStr) -> *mut BeskidArray {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if len == 0 {
        return array_new(1, 0);
    }
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let target = array_new(1, len);
    if target.is_null() {
        panic!("bytes_from_str allocation failed");
    }
    let dst_ptr = unsafe { (*target).ptr };
    if dst_ptr.is_null() {
        return target;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst_ptr, len);
    }
    target
}

/// Validate UTF-8 in `u8[]` and allocate a `BeskidStr` header (panics on invalid UTF-8).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_from_bytes_utf8(value: *const BeskidArray) -> *mut BeskidStr {
    if value.is_null() {
        panic!("null array handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if len == 0 {
        static Z: [u8; 1] = [0];
        return str_new(Z.as_ptr(), 0);
    }
    if ptr.is_null() {
        panic!("null array data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    if std::str::from_utf8(bytes).is_err() {
        panic!("invalid utf-8 bytes");
    }
    str_new(ptr, len)
}

/// Memcpy `len` bytes from `src` at `src_off` into `dst` at `dst_off`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bytes_copy(
    dst: *const BeskidArray,
    dst_off: i64,
    src: *const BeskidArray,
    src_off: i64,
    len: i64,
) {
    if dst.is_null() || src.is_null() {
        panic!("null array handle");
    }
    if len < 0 {
        panic!("negative copy length");
    }
    let len = len as usize;
    if len == 0 {
        return;
    }
    let dst_off = dst_off.max(0) as usize;
    let src_off = src_off.max(0) as usize;
    let (dst_ptr, dst_len) = unsafe { ((*dst).ptr, (*dst).len) };
    let (src_ptr, src_len) = unsafe { ((*src).ptr, (*src).len) };
    if dst_ptr.is_null() || src_ptr.is_null() {
        panic!("null array data");
    }
    if dst_off.saturating_add(len) > dst_len || src_off.saturating_add(len) > src_len {
        panic!("bytes_copy out of bounds");
    }
    unsafe {
        std::ptr::copy_nonoverlapping(src_ptr.add(src_off), dst_ptr.add(dst_off), len);
    }
}

/// Returns the byte at `index` (traps when out of bounds).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bytes_get(value: *const BeskidArray, index: i64) -> i64 {
    if value.is_null() {
        panic!("null array handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if index < 0 || index as usize >= len {
        panic!("bytes_get out of bounds");
    }
    if ptr.is_null() {
        panic!("null array data");
    }
    unsafe { *ptr.add(index as usize) as i64 }
}

/// Stores `byte` at `index` (traps when out of bounds); returns the array handle.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bytes_set(value: *const BeskidArray, index: i64, byte: i64) -> *const BeskidArray {
    if value.is_null() {
        panic!("null array handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if index < 0 || index as usize >= len {
        panic!("bytes_set out of bounds");
    }
    if ptr.is_null() {
        panic!("null array data");
    }
    unsafe {
        *ptr.add(index as usize) = byte as u8;
    }
    value
}

/// Lexicographic compare; returns `-1`, `0`, or `1`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn bytes_compare(left: *const BeskidArray, right: *const BeskidArray) -> i64 {
    if left.is_null() || right.is_null() {
        panic!("null array handle");
    }
    let (left_ptr, left_len) = unsafe { ((*left).ptr, (*left).len) };
    let (right_ptr, right_len) = unsafe { ((*right).ptr, (*right).len) };
    let left_slice = if left_ptr.is_null() || left_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(left_ptr, left_len) }
    };
    let right_slice = if right_ptr.is_null() || right_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(right_ptr, right_len) }
    };
    match left_slice.cmp(right_slice) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Allocate a `u8[]` copy of bytes read from raw parts (internal helper for syscall_read_bytes).
pub(crate) fn array_from_vec(bytes: Vec<u8>) -> *mut BeskidArray {
    let len = bytes.len();
    let target = array_new(1, len);
    if target.is_null() {
        panic!("array_from_vec allocation failed");
    }
    if len == 0 {
        return target;
    }
    let dst_ptr = unsafe { (*target).ptr };
    if dst_ptr.is_null() {
        return target;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst_ptr, len);
    }
    target
}
