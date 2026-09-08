//! `beskid publish` — release build with plain step output and registry-oriented defaults.

use crate::commands::build::{self, BuildArgs};
use anyhow::Result;
use clap::Args;

/// Build release artifacts for project targets detected from the project model.
#[derive(Args, Debug)]
pub struct PublishArgs {
    /// Build options inherited from `beskid dev build compile`, with `--release` enabled automatically.
    #[command(flatten)]
    pub build: BuildArgs,
}

/// Execute a release build for the detected target without spinner UI.
pub fn execute(mut args: PublishArgs) -> Result<()> {
    args.build.release = true;
    args.build.plain = true;
    build::execute(args.build)
}
