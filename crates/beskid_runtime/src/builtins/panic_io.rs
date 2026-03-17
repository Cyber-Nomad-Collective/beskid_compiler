use beskid_abi::BeskidStr;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn write_stdout(bytes: &[u8]) {
    use std::arch::asm;

    if bytes.is_empty() {
        return;
    }

    let mut written = 0usize;
    while written < bytes.len() {
        let ptr = unsafe { bytes.as_ptr().add(written) };
        let len = bytes.len() - written;
        let mut result: isize;
        unsafe {
            asm!(
                "syscall",
                in("rax") 1usize,
                in("rdi") 1usize,
                in("rsi") ptr,
                in("rdx") len,
                lateout("rax") result,
                lateout("rcx") _,
                lateout("r11") _,
            );
        }
        if result <= 0 {
            break;
        }
        written += result as usize;
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn write_stdout(bytes: &[u8]) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(bytes);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn panic(_msg_ptr: *const u8, _msg_len: usize) -> ! {
    panic!("beskid panic");
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn sys_print(value: *const BeskidStr) {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    write_stdout(bytes);
}

#[unsafe(no_mangle)]
pub extern "C-unwind" fn sys_println(value: *const BeskidStr) {
    if value.is_null() {
        panic!("null string handle");
    }
    let (ptr, len) = unsafe { ((*value).ptr, (*value).len) };
    if ptr.is_null() {
        panic!("null string data");
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    write_stdout(bytes);
    write_stdout(b"\n");
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
