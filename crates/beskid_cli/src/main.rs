//! `beskid` binary entry: initializes logging and delegates to [`cli::run`].

pub mod cli;
pub mod commands;
pub mod corelib_runtime;
pub mod errors;
pub mod frontend;
pub mod pipeline_ui;
pub mod project_args;

fn main() -> miette::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    cli::run()
}
