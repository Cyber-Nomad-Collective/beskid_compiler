pub mod build_ui;
pub mod cli;
pub mod commands;
pub mod errors;
pub mod frontend;
pub mod stdlib_runtime;

fn main() -> miette::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    cli::run()
}
