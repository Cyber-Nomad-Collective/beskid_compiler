use beskid_abi::BeskidStr;

use super::{alloc::alloc, strings::str_new};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn linux_write_fd(fd: i32, mut ptr: *const u8, mut len: usize) -> i64 {
    use std::arch::asm;

    if len == 0 {
        return 0;
    }

    let mut written_total = 0i64;
    while len > 0 {
        let mut result: isize;
        unsafe {
            asm!(
                "syscall",
                in("rax") 1usize,
                in("rdi") fd as usize,
                in("rsi") ptr,
                in("rdx") len,
                lateout("rax") result,
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        if result <= 0 {
            if written_total > 0 {
                return written_total;
            }
            return -1;
        }
        let chunk = result as usize;
        written_total += chunk as i64;
        ptr = unsafe { ptr.add(chunk) };
        len -= chunk;
    }
    written_total
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn write_fd_bytes(fd: i64, bytes: &[u8]) -> i64 {
    if bytes.is_empty() {
        return 0;
    }
    if fd < 0 || fd > i32::MAX as i64 {
        return -1;
    }
    linux_write_fd(fd as i32, bytes.as_ptr(), bytes.len())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn write_fd_bytes(fd: i64, bytes: &[u8]) -> i64 {
    use std::io::Write;
    if bytes.is_empty() {
        return 0;
    }
    match fd {
        1 => std::io::stdout()
            .write(bytes)
            .map(|n| n as i64)
            .unwrap_or(-1),
        2 => std::io::stderr()
            .write(bytes)
            .map(|n| n as i64)
            .unwrap_or(-1),
        _ => -1,
    }
}

fn empty_string_handle() -> *mut BeskidStr {
    str_new(b"".as_ptr(), 0)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn read_fd_bytes(fd: i64, max_bytes: i64) -> *mut BeskidStr {
    use std::arch::asm;
    if fd < 0 || fd > i32::MAX as i64 || max_bytes <= 0 {
        return empty_string_handle();
    }
    let cap = max_bytes as usize;
    let buffer = alloc(cap, std::ptr::null()).cast::<u8>();
    if buffer.is_null() {
        return empty_string_handle();
    }
    let mut result: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 0usize,
            in("rdi") fd as usize,
            in("rsi") buffer,
            in("rdx") cap,
            lateout("rax") result,
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    if result <= 0 {
        return empty_string_handle();
    }
    str_new(buffer, result as usize)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn read_fd_bytes(fd: i64, max_bytes: i64) -> *mut BeskidStr {
    use std::io::Read;
    if fd != 0 || max_bytes <= 0 {
        return empty_string_handle();
    }
    let cap = max_bytes as usize;
    let buffer = alloc(cap, std::ptr::null()).cast::<u8>();
    if buffer.is_null() {
        return empty_string_handle();
    }
    let target = unsafe { std::slice::from_raw_parts_mut(buffer, cap) };
    let read = std::io::stdin().read(target).unwrap_or(0);
    if read == 0 {
        return empty_string_handle();
    }
    str_new(buffer, read)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn panic(_msg_ptr: *const u8, _msg_len: usize) -> ! {
    panic!("beskid panic");
}

/// Cross-platform write of UTF-8 string payload to `fd`.
/// Returns bytes written, or `-1` on error / unsupported `fd` (non-Linux: only `1` and `2`).
#[unsafe(no_mangle)]
pub extern "C-unwind" fn syscall_write(fd: i64, value: *const BeskidStr) -> i64 {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    write_fd_bytes(fd, bytes)
}

/// Cross-platform read of UTF-8 payload from `fd`.
/// Returns an empty string when no bytes are available or `fd` is unsupported.
#[unsafe(no_mangle)]
pub extern "C-unwind" fn syscall_read(fd: i64, max_bytes: i64) -> *mut BeskidStr {
    read_fd_bytes(fd, max_bytes)
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn panic_str(value: *const BeskidStr) -> ! {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = std::str::from_utf8(bytes).unwrap_or("<invalid utf8>");
    panic!("{text}");
}
