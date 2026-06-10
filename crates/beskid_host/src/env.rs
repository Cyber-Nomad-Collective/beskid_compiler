//! Environment host builtins for corelib `Core.Environment`.

use beskid_abi::BeskidStr;

use crate::strings::{read_string_path, string_from_rust};

/// Environment variable lookup. Returns empty string when unset.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn env_get(name: *const BeskidStr) -> *mut BeskidStr {
    let name = read_string_path(name);
    match std::env::var(&name) {
        Ok(value) => string_from_rust(&value),
        Err(_) => string_from_rust(""),
    }
}

/// Set environment variable. Returns `0` on success, `-1` on failure.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn env_set(name: *const BeskidStr, value: *const BeskidStr) -> i64 {
    let name = read_string_path(name);
    let value = read_string_path(value);
    unsafe {
        std::env::set_var(&name, &value);
    }
    0
}

/// Current working directory. Returns `"."` when unavailable.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn env_getcwd() -> *mut BeskidStr {
    match std::env::current_dir() {
        Ok(path) => string_from_rust(&path.to_string_lossy()),
        Err(_) => string_from_rust("."),
    }
}
