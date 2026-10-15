use crate::commands::{
    build::BuildArgs, clif::ClifArgs, compiler_mod::ModArgs, corelib::CorelibArgs, fetch::FetchArgs, graph::GraphArgs,
    import::ImportArgs, lock::LockArgs, lsp::LspArgs, migrate_bsol::MigrateBsolArgs, parse::ParseArgs, repl::ReplArgs,
    runtime_kit::RuntimeKitArgs, tree::TreeArgs, validate_bsol::ValidateBsolArgs,
};
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct DevArgs {
    /// Emit backend logs for compiler inspection.
    #[arg(long, env = "BESKID_LOG_CRANELIFT")]
    pub log_cranelift: bool,
    #[command(subcommand)]
    pub(super) command: DevCommand,
}
#[derive(Subcommand, Debug)]
pub(super) enum DevCommand {
    /// Private isolated native Mod worker protocol.
    #[command(hide = true)]
    NativeModWorker(NativeModWorkerArgs),
    /// Syntax inspection.
    Syntax(DevSyntaxArgs),
    /// Backend output and linking inspection.
    Build(BuildArgs),
    /// Dependency plumbing and graph inspection.
    Project(DevProjectArgs),
    /// Materialize the bundled Corelib workspace.
    Corelib(CorelibArgs),
    /// Construct and validate ABI-v5 runtime kits.
    RuntimeKit(RuntimeKitArgs),
    /// Compiler Mod artifact lifecycle.
    Mod(ModArgs),
    /// Foreign boundary metadata authoring.
    Import(ImportArgs),
    /// Serve or install the language server.
    Lsp(LspArgs),
    /// BSOL maintenance.
    Bsol(DevBsolArgs),
    /// Experimental snippet evaluator.
    Repl(ReplArgs),
    /// Validate and stamp this executable's private installation owner.
    ToolchainOwner(crate::commands::toolchain_owner::ToolchainOwnerArgs),
    /// Machine-readable canonical CLI capabilities.
    Capabilities(CapabilitiesArgs),
}
#[derive(Args, Debug)]
pub(super) struct DevSyntaxArgs {
    #[command(subcommand)]
    pub(super) command: DevSyntaxCommand,
}
#[derive(Subcommand, Debug)]
pub(super) enum DevSyntaxCommand {
    Parse(ParseArgs),
    Tree(TreeArgs),
    Clif(ClifArgs),
}
#[derive(Args, Debug)]
pub(super) struct DevProjectArgs {
    #[command(subcommand)]
    pub(super) command: DevProjectCommand,
}
#[derive(Subcommand, Debug)]
pub(super) enum DevProjectCommand {
    Fetch(FetchArgs),
    Lock(LockArgs),
    Graph(GraphArgs),
}
#[derive(Args, Debug)]
pub(super) struct DevBsolArgs {
    #[command(subcommand)]
    pub(super) command: DevBsolCommand,
}
#[derive(Subcommand, Debug)]
pub(super) enum DevBsolCommand {
    Validate(ValidateBsolArgs),
    Migrate(MigrateBsolArgs),
}
#[derive(Args, Debug)]
pub(super) struct CapabilitiesArgs {
    /// Emit the schema-1 capability document.
    #[arg(long, required = true)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub(super) struct NativeModWorkerArgs {
    #[arg(long)]
    pub endpoint: std::path::PathBuf,
}
