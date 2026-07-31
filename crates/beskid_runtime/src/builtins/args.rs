//! Process argument soft builtins declared in `runtime_manifest.bsol`.

use beskid_abi::BeskidStr;

use super::{alloc::alloc, strings::str_new};

/// Number of arguments exposed to the currently executing Beskid program, including argv[0].
#[unsafe(no_mangle)]
pub extern "C-unwind" fn args_count() -> i64 {
    i64::try_from(std::env::args_os().count()).unwrap_or(i64::MAX)
}

/// Return an owned Beskid string for `argv[index]`, or null when the index is out of range.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn args_get(index: i64) -> *mut BeskidStr {
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    let Some(argument) = std::env::args().nth(index) else {
        return std::ptr::null_mut();
    };
    let bytes = argument.as_bytes();
    let buffer = alloc(bytes.len(), std::ptr::null()).cast::<u8>();
    if buffer.is_null() {
        panic!("args_get string allocation failed");
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    }
    str_new(buffer, bytes.len())
}
