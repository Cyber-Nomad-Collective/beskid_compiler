//! Process host builtins for corelib `Core.Process`.

/// Process id.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn process_getpid() -> i64 {
    std::process::id() as i64
}

/// Terminate process with exit code (never returns).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn process_exit(code: i64) -> ! {
    std::process::exit(code as i32);
}
