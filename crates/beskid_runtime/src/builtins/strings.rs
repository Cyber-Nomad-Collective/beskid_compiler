use beskid_abi::BeskidStr;

use super::alloc::alloc;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_new(ptr: *const u8, len: usize) -> *mut BeskidStr {
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

#[unsafe(no_mangle)]
pub extern "C-unwind" fn str_len(value: *const BeskidStr) -> usize {
    if value.is_null() {
        panic!("null string handle");
    }
    unsafe { (*value).len }
}

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

    str_new(buffer.cast::<u8>(), total_len)
}
