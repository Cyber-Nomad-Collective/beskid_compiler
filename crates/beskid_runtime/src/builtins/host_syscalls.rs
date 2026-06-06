//! Host OS helpers for corelib `System.*` modules (runtime-owned; not user `Extern`).

use beskid_abi::BeskidStr;

use super::strings::str_new;

fn string_from_rust(text: &str) -> *mut BeskidStr {
    if text.is_empty() {
        static Z: [u8; 1] = [0];
        return str_new(Z.as_ptr(), 0);
    }
    str_new(text.as_ptr(), text.len())
}

fn read_string_path(value: *const BeskidStr) -> String {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if len == 0 {
        return String::new();
    }
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes)
        .unwrap_or_else(|_| panic!("invalid utf-8 path"))
        .to_string()
}

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

/// UTC wall-clock nanoseconds since Unix epoch (best effort).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn clock_realtime_nanos() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Monotonic nanoseconds (best effort).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn clock_monotonic_nanos() -> i64 {
    use std::time::Instant;
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_nanos() as i64
}

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn linux_tty_winsize(fd: i32) -> i64 {
    use std::arch::asm;
    let mut ws = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let request: u64 = 0x5413; // TIOCGWINSZ
    let mut result: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 16usize,
            in("rdi") fd as usize,
            in("rsi") request,
            in("rdx") &mut ws as *mut Winsize,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    if result != 0 || ws.ws_col == 0 || ws.ws_row == 0 {
        return 0;
    }
    ((ws.ws_col as i64) << 16) | (ws.ws_row as i64)
}

/// Terminal size packed as `(columns << 16) | rows`, or `0` when unavailable.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn tty_winsize(fd: i64) -> i64 {
    if fd < 0 || fd > i32::MAX as i64 {
        return 0;
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return linux_tty_winsize(fd as i32);
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = fd;
        0
    }
}
