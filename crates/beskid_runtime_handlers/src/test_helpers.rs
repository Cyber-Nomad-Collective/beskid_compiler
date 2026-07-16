static BYTES: &[u8] = b"lang-handler";

/// Stable pointer for contract tests (called via dispatch wrapper only).
pub fn test_bytes_ptr() -> i64 {
    BYTES.as_ptr() as i64
}

/// Length of the buffer returned by [`test_bytes_ptr`].
pub fn test_bytes_len() -> i64 {
    BYTES.len() as i64
}
