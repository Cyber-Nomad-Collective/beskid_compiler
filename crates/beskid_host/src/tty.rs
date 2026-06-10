//! Terminal geometry host builtin for corelib console.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
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
        linux_tty_winsize(fd as i32)
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = fd;
        0
    }
}
