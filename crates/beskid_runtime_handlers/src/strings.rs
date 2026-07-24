use beskid_abi::BeskidStr;

/// Content equality comparison for two `BeskidStr` values.
pub fn str_eq(left: *const BeskidStr, right: *const BeskidStr) -> i64 {
    if left.is_null() || right.is_null() {
        panic!("null string handle");
    }

    let (left_ptr, left_len) = unsafe { ((*left).ptr, (*left).len) };
    let (right_ptr, right_len) = unsafe { ((*right).ptr, (*right).len) };

    if left_ptr == right_ptr && left_len == right_len {
        return 1;
    }

    if left_len == right_len {
        if left_ptr.is_null() || right_ptr.is_null() {
            return i64::from(left_len == 0);
        }
        let equal = unsafe {
            std::slice::from_raw_parts(left_ptr, left_len) == std::slice::from_raw_parts(right_ptr, right_len)
        };
        return i64::from(equal);
    }

    0
}
