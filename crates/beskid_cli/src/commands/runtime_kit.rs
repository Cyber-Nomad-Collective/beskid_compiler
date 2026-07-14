//! `beskid runtime-kit build` — publish one exact ABI-v5 target/profile kit.

use std::path::PathBuf;

use anyhow::Result;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitBuildOptions, RuntimeKitProfile, build};
use clap::{Args, Subcommand, ValueEnum};

#[derive(Args, Debug)]
pub struct RuntimeKitArgs {
    #[command(subcommand)]
    pub command: RuntimeKitCommand,
}

#[derive(Subcommand, Debug)]
pub enum RuntimeKitCommand {
    /// Validate and atomically publish prebuilt native runtime artifacts.
    Build(RuntimeKitBuildArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RuntimeKitProfileArg {
    Debug,
    Release,
}

impl RuntimeKitProfileArg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

impl From<RuntimeKitProfileArg> for RuntimeKitProfile {
    fn from(value: RuntimeKitProfileArg) -> Self {
        match value {
            RuntimeKitProfileArg::Debug => Self::Debug,
            RuntimeKitProfileArg::Release => Self::Release,
        }
    }
}

#[derive(Args, Debug)]
pub struct RuntimeKitBuildArgs {
    /// Installation prefix containing `lib/beskid-runtime/abi-5/`.
    #[arg(long)]
    pub prefix: PathBuf,

    /// Exact supported target triple.
    #[arg(long)]
    pub target: String,

    /// Runtime optimization/diagnostic profile.
    #[arg(long, value_enum)]
    pub profile: RuntimeKitProfileArg,

    /// Canonical SHA-256 of the hosted Beskid runtime source corpus.
    #[arg(long)]
    pub source_hash: String,

    /// Prebuilt target static library.
    #[arg(long)]
    pub static_library: PathBuf,

    /// Prebuilt target shared library.
    #[arg(long)]
    pub shared_library: PathBuf,

    /// Windows shared import library; required only for the Windows target.
    #[arg(long)]
    pub shared_import_library: Option<PathBuf>,
}

pub fn execute(args: RuntimeKitArgs) -> Result<()> {
    let RuntimeKitCommand::Build(args) = args.command;
    let built = build(RuntimeKitBuildOptions {
        prefix: args.prefix,
        target: args.target,
        profile: args.profile.into(),
        source_hash: args.source_hash,
        static_library: args.static_library,
        shared_library: args.shared_library,
        shared_import_library: args.shared_import_library,
    })?;
    println!("Built ABI-v5 runtime kit at {}", built.root.display());
    Ok(())
}
