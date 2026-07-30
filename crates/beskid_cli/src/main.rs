//! `beskid` binary entry: initializes logging and delegates to [`cli::run`].
//!
//! Dispatch runs on an explicitly sized worker thread because the loader-provided main-thread stack
//! is too small for canonical corpus compilation on some hosts (Windows MSVC reserves 1 MiB, and
//! `RUST_MIN_STACK` cannot grow the main thread). See [`beskid_tools::run_on_compiler_stack`].

pub mod cli;
pub mod commands;
pub mod project_args;

fn main() {
    beskid_tools::run_on_compiler_stack(|| {
        if let Err(report) = cli::run() {
            beskid_tools::print_report(&report);
            std::process::exit(1);
        }
    });
}
