//! `beskid` binary entry: initializes logging and delegates to [`cli::run`].

pub mod cli;
pub mod commands;
pub mod corelib_runtime;
pub mod errors;
pub mod frontend;
pub mod logging;
pub mod pipeline_ui;
pub mod project_args;
pub mod toolchain;

fn main() {
    if let Err(report) = cli::run() {
        crate::errors::print_report(&report);
        std::process::exit(1);
    }
}
