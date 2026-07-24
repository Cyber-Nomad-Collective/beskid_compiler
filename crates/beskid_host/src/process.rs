//! Process host builtins for corelib `Core.Process`.

use std::sync::atomic::AtomicI32;
use std::process::Command;

static LAST_EXIT_CODE: AtomicI32 = AtomicI32::new(0);

/// Process id.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn process_getpid() -> i64 {
    std::process::id() as i64
}

/// Terminate process with exit code (never returns). Stores code for `process_last_exit_code`.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn process_exit(code: i64) -> ! {
    LAST_EXIT_CODE.store(code as i32, std::sync::atomic::Ordering::Relaxed);
    std::process::exit(code as i32);
}

/// Returns the last exit code set by `process_exit`, or `0` if none.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn process_last_exit_code() -> i64 {
    LAST_EXIT_CODE.load(std::sync::atomic::Ordering::Relaxed) as i64
}

/// Spawns a child process via shell and waits for completion.
/// Returns exit code on success (0–255), or -1 on failure.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn process_run(command: *const beskid_abi::BeskidStr) -> i64 {
    let cmd_bytes = crate::strings::read_string_path(command);
    if cmd_bytes.is_empty() {
        return -1;
    }
    #[cfg(target_family = "unix")]
    let status = Command::new("sh").arg("-c").arg(&cmd_bytes).status();
    #[cfg(target_family = "windows")]
    let status = Command::new("cmd").arg("/C").arg(&cmd_bytes).status();
    match status {
        Ok(s) => s.code().unwrap_or(-1) as i64,
        Err(_) => -1,
    }
}
