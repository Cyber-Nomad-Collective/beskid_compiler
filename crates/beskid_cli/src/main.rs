//! `beskid` binary entry: initializes logging and delegates to [`cli::run`].
//!
//! Dispatch runs on an explicitly sized worker thread because the loader-provided main-thread stack
//! is too small for canonical corpus compilation on some hosts (Windows MSVC reserves 1 MiB, and
//! `RUST_MIN_STACK` cannot grow the main thread). See [`beskid_tools::run_on_compiler_stack`].

pub mod cli;
pub mod commands;
pub mod machine_output;
pub mod project_args;

/// Restore the default `SIGPIPE` action. Rust starts processes with it ignored, so writing to a
/// closed pipe (`beskid run | head`) panics inside `println!`; with the default action the shell
/// sees the conventional quiet status 141 and the program the CLI starts inherits normal Unix pipe
/// behaviour.
#[cfg(unix)]
fn restore_default_sigpipe() {
    // SAFETY: called first in `main`, before any thread starts; `signal` with `SIG_DFL` only resets
    // the disposition of one signal and touches no Rust-managed state.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_default_sigpipe() {}

fn main() {
    restore_default_sigpipe();
    beskid_tools::run_on_compiler_stack(|| {
        if let Err(report) = cli::run() {
            beskid_tools::print_report(&report);
            std::process::exit(1);
        }
    });
}
