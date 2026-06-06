//! Filesystem host builtins for corelib `System.FS`.

use beskid_abi::BeskidStr;

use crate::strings::{read_string_path, string_from_rust};

/// Read entire file as UTF-8 text. Returns empty string when missing or unreadable.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn fs_read_text(path: *const BeskidStr) -> *mut BeskidStr {
    let path = read_string_path(path);
    match std::fs::read_to_string(&path) {
        Ok(text) => string_from_rust(&text),
        Err(_) => string_from_rust(""),
    }
}

/// Write UTF-8 text to a path. Returns `0` on success, `-1` on failure.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn fs_write_text(path: *const BeskidStr, data: *const BeskidStr) -> i64 {
    let path = read_string_path(path);
    if data.is_null() {
        return -1;
    }
    let (ptr, len) = unsafe { ((*data).ptr, (*data).len) };
    let bytes = if ptr.is_null() || len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    match std::fs::write(&path, bytes) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Returns `1` when the path exists, `0` otherwise.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn fs_exists(path: *const BeskidStr) -> i64 {
    let path = read_string_path(path);
    i64::from(std::path::Path::new(&path).exists())
}

/// Delete file or directory. Returns `0` on success, `-1` on failure.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn fs_delete(path: *const BeskidStr) -> i64 {
    let path = read_string_path(path);
    let p = std::path::Path::new(&path);
    let result = if p.is_dir() {
        std::fs::remove_dir(p)
    } else {
        std::fs::remove_file(p)
    };
    match result {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Create directory (non-recursive). Returns `0` on success, `-1` on failure.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn fs_mkdir(path: *const BeskidStr) -> i64 {
    let path = read_string_path(path);
    match std::fs::create_dir(&path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}
