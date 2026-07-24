use beskid_up::{UpArgs, UpCommand, execute};
use clap::Parser;

#[derive(Parser)]
#[command(name = "beskid-up")]
struct Cli {
    #[command(subcommand)]
    command: UpCommand,
}

fn main() {
    if let Err(error) = execute(UpArgs { command: Cli::parse().command }) {
        eprintln!("beskid-up: {error}");
        std::process::exit(1);
    }
}
