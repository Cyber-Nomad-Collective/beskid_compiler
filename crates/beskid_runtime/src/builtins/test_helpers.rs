static BYTES: &[u8] = b"Hello from libc.write\n";

#[unsafe(no_mangle)]
pub extern "C-unwind" fn test_bytes_ptr() -> u64 {
    BYTES.as_ptr() as u64
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn test_bytes_len() -> u64 {
    BYTES.len() as u64
}

