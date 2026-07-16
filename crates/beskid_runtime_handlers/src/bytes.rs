use beskid_abi::BeskidArray;

/// Returns the byte at `index` (traps when out of bounds).
pub fn bytes_get(value: *const BeskidArray, index: i64) -> i64 {
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

/// Lexicographic compare for byte arrays; returns `-1`, `0`, or `1`.
pub fn bytes_compare(left: *const BeskidArray, right: *const BeskidArray) -> i64 {
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
