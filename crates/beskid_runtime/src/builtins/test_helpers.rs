static BYTES: &[u8] = b"Hello from libc.write\n";

/// Stable test payload pointer cast to `u64` for builtin smoke tests.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn test_bytes_ptr() -> u64 {
    BYTES.as_ptr() as u64
}

/// Length of the buffer returned by [`test_bytes_ptr`].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn test_bytes_len() -> u64 {
    BYTES.len() as u64
}
