fn main() {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() == Some(std::ffi::OsStr::new("--version")) && arguments.next().is_none() {
        println!("beskid_lsp {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    limit_malloc_arenas();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap_or_else(|error| {
        eprintln!("cannot start the async runtime: {error}");
        std::process::exit(1);
    });
    if let Err(error) = runtime.block_on(beskid_lsp::run_stdio_server()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// glibc gives each thread that allocates concurrently its own malloc arena (up to eight per
/// core) and returns little of their freed memory. The server allocates and frees whole typed
/// programs on many threads, so its resident size grew to twice what it used (measured in a
/// Corelib soak: 12 GB with the default arenas, 5.6 GB with two, at the same latency).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn limit_malloc_arenas() {
    // SAFETY: `mallopt` only adjusts allocator tuning, and it runs before the runtime starts any
    // other thread.
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn limit_malloc_arenas() {}
