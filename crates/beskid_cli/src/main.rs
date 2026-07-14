//! `beskid` binary entry: initializes logging and delegates to [`cli::run`].

pub mod cli;
pub mod commands;
pub mod project_args;

fn main() {
    if let Err(report) = cli::run() {
        beskid_tools::print_report(&report);
        std::process::exit(1);
    }
}
