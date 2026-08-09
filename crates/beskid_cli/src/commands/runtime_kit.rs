//! `beskid runtime-kit build` — publish one exact ABI-v5 target/profile kit.

use std::path::PathBuf;

use anyhow::Result;
use beskid_tools::toolchain::runtime_kit::{
    RuntimeKitBuildOptions, RuntimeKitMatrixBuildOptions, RuntimeKitProfile, RuntimeKitProfileArtifacts, build,
    build_matrix, build_native_host,
};
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
    /// Build and publish the canonical runtime for this exact native host.
    BuildNativeHost(RuntimeKitNativeHostBuildArgs),
    /// Publish the required debug and release artifacts for one target.
    BuildMatrix(RuntimeKitBuildMatrixArgs),
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

#[derive(Args, Debug)]
pub struct RuntimeKitNativeHostBuildArgs {
    /// Empty installation prefix receiving `lib/beskid-runtime/abi-5/`.
    #[arg(long)]
    pub prefix: PathBuf,

    /// Runtime optimization/diagnostic profile for this host build.
    #[arg(long, value_enum)]
    pub profile: RuntimeKitProfileArg,
}

#[derive(Args, Debug)]
pub struct RuntimeKitBuildMatrixArgs {
    /// Installation prefix containing `lib/beskid-runtime/abi-5/`.
    #[arg(long)]
    pub prefix: PathBuf,

    /// Exact supported target triple.
    #[arg(long)]
    pub target: String,

    /// Debug static library emitted by the canonical runtime build.
    #[arg(long)]
    pub debug_static_library: PathBuf,

    /// Debug shared library emitted by the canonical runtime build.
    #[arg(long)]
    pub debug_shared_library: PathBuf,

    /// Release static library emitted by the canonical runtime build.
    #[arg(long)]
    pub release_static_library: PathBuf,

    /// Release shared library emitted by the canonical runtime build.
    #[arg(long)]
    pub release_shared_library: PathBuf,

    /// Debug static-archive symbol list emitted by the platform provenance adapter.
    #[arg(long)]
    pub debug_static_provenance_symbol_list: PathBuf,

    /// Debug shared-library symbol list emitted by the platform provenance adapter.
    #[arg(long)]
    pub debug_shared_provenance_symbol_list: PathBuf,

    /// Release static-archive symbol list emitted by the platform provenance adapter.
    #[arg(long)]
    pub release_static_provenance_symbol_list: PathBuf,

    /// Release shared-library symbol list emitted by the platform provenance adapter.
    #[arg(long)]
    pub release_shared_provenance_symbol_list: PathBuf,

    /// Debug Windows import library; required only for the Windows target.
    #[arg(long)]
    pub debug_shared_import_library: Option<PathBuf>,

    /// Release Windows import library; required only for the Windows target.
    #[arg(long)]
    pub release_shared_import_library: Option<PathBuf>,
}

pub fn execute(args: RuntimeKitArgs) -> Result<()> {
    match args.command {
        RuntimeKitCommand::Build(args) => {
            let built = build(RuntimeKitBuildOptions {
                prefix: args.prefix,
                target: args.target,
                profile: args.profile.into(),
                static_library: args.static_library,
                shared_library: args.shared_library,
                shared_import_library: args.shared_import_library,
            })?;
            println!("Built ABI-v5 runtime kit at {}", built.root.display());
        }
        RuntimeKitCommand::BuildNativeHost(args) => {
            let built = build_native_host(args.prefix, args.profile.into())?;
            println!("Built native-host ABI-v5 runtime kit at {}", built.root.display());
        }
        RuntimeKitCommand::BuildMatrix(args) => {
            let built = build_matrix(RuntimeKitMatrixBuildOptions {
                prefix: args.prefix,
                target: args.target,
                profiles: vec![
                    RuntimeKitProfileArtifacts {
                        profile: RuntimeKitProfile::Debug,
                        static_library: args.debug_static_library,
                        shared_library: args.debug_shared_library,
                        shared_import_library: args.debug_shared_import_library,
                        static_provenance_symbol_list: args.debug_static_provenance_symbol_list,
                        shared_provenance_symbol_list: args.debug_shared_provenance_symbol_list,
                    },
                    RuntimeKitProfileArtifacts {
                        profile: RuntimeKitProfile::Release,
                        static_library: args.release_static_library,
                        shared_library: args.release_shared_library,
                        shared_import_library: args.release_shared_import_library,
                        static_provenance_symbol_list: args.release_static_provenance_symbol_list,
                        shared_provenance_symbol_list: args.release_shared_provenance_symbol_list,
                    },
                ],
            })?;
            for kit in built {
                println!("Built ABI-v5 runtime kit at {}", kit.root.display());
            }
        }
    }
    Ok(())
}
