//! Process entry helpers for the toolchain binaries.
//!
//! Compiling the canonical corpus recurses deeply through parse, lowering, and ISLE emission, and
//! the OS-provided main-thread stack is not sized for it: Windows MSVC reserves 1 MiB and
//! `RUST_MIN_STACK` cannot grow the main thread on any platform. Binaries therefore run their real
//! work on an explicitly sized worker thread instead of the stack the loader handed them.

/// Floor for the worker stack that drives compilation.
///
/// Corelib lowering already needs several MiB (see the 8 MiB integration-test stack), so this keeps
/// generous headroom for the deepest canonical corpus emission rather than tracking observed peaks.
pub const COMPILER_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Resolve the worker stack size, honouring `RUST_MIN_STACK` only when it asks for more room.
pub fn compiler_stack_size() -> usize {
    resolve_compiler_stack_size(std::env::var("RUST_MIN_STACK").ok().as_deref())
}

fn resolve_compiler_stack_size(requested: Option<&str>) -> usize {
    requested
        .and_then(|value| value.trim().parse::<usize>().ok())
        .map_or(COMPILER_STACK_SIZE, |size| size.max(COMPILER_STACK_SIZE))
}

/// Run a binary's entry closure on a worker thread with a stack sized for compilation.
///
/// Panics propagate to the caller unchanged, and a worker that cannot be spawned fails closed: the
/// entry point must never silently fall back to the undersized main-thread stack.
pub fn run_on_compiler_stack<F>(entry: F)
where
    F: FnOnce() + Send + 'static,
{
    let worker =
        std::thread::Builder::new().name("beskid-main".to_owned()).stack_size(compiler_stack_size()).spawn(entry);
    let worker = match worker {
        Ok(worker) => worker,
        Err(error) => {
            eprintln!("failed to spawn the beskid worker thread: {error}");
            std::process::exit(1);
        }
    };
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn unset_rust_min_stack_uses_the_compiler_floor() {
        assert_eq!(resolve_compiler_stack_size(None), COMPILER_STACK_SIZE);
    }

    #[test]
    fn smaller_or_unparsable_rust_min_stack_never_lowers_the_floor() {
        assert_eq!(resolve_compiler_stack_size(Some("1048576")), COMPILER_STACK_SIZE);
        assert_eq!(resolve_compiler_stack_size(Some("0")), COMPILER_STACK_SIZE);
        assert_eq!(resolve_compiler_stack_size(Some("plenty")), COMPILER_STACK_SIZE);
        assert_eq!(resolve_compiler_stack_size(Some("")), COMPILER_STACK_SIZE);
    }

    #[test]
    fn larger_rust_min_stack_wins() {
        let requested = COMPILER_STACK_SIZE * 2;
        assert_eq!(resolve_compiler_stack_size(Some(&format!(" {requested} "))), requested);
    }

    #[test]
    fn worker_stack_outgrows_the_windows_main_thread_reserve() {
        // 8 MiB of frames overflows both the Windows 1 MiB main-thread reserve and the 2 MiB
        // default thread stack, so completing here proves the worker owns the requested stack.
        fn burn(depth: usize) -> usize {
            let mut frame = [0_u8; 64 * 1024];
            frame[depth % frame.len()] = depth as u8;
            let consumed = std::hint::black_box(&frame).len();
            if depth == 0 { consumed } else { consumed + burn(depth - 1) }
        }

        let observed = Arc::new(AtomicUsize::new(0));
        let recorded = Arc::clone(&observed);
        run_on_compiler_stack(move || {
            recorded.store(burn(127), Ordering::SeqCst);
        });
        assert_eq!(observed.load(Ordering::SeqCst), 128 * 64 * 1024);
    }
}
