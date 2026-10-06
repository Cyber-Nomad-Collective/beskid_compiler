use anyhow::Result;
use clap::{Args, Subcommand};
#[derive(Args, Debug)]
pub struct ToolchainArgs {
    #[command(subcommand)]
    pub command: ToolchainCommand,
}
#[derive(Subcommand, Debug)]
pub enum ToolchainCommand {
    Status,
    Update,
}
pub fn execute(args: ToolchainArgs) -> Result<()> {
    match args.command {
        ToolchainCommand::Status => {
            println!("{}", beskid_up::CurrentInstallationStatus()?);
            Ok(())
        }
        ToolchainCommand::Update => beskid_up::UpdateConfiguredToolchain().map(|_| ()).map_err(Into::into),
    }
}
