use crate::commands::analyze::AnalyzeArgs;
use crate::commands::build::BuildArgs;
use crate::commands::clif::ClifArgs;
use crate::commands::corelib::CorelibArgs;
use crate::commands::doc::DocArgs;
use crate::commands::fetch::FetchArgs;
use crate::commands::format::FormatArgs;
use crate::commands::graph::GraphArgs;
use crate::commands::lock::LockArgs;
use crate::commands::parse::ParseArgs;
use crate::commands::test::TestArgs;
use crate::commands::tree::TreeArgs;
use crate::commands::update::UpdateArgs;
use beskid_pckg::PckgArgs;
use clap::Subcommand;

/// Developer command groups used by the documentation and automation examples.
#[derive(clap::Args, Debug)]
pub struct DevArgs {
    #[command(subcommand)]
    pub(super) command: DevCommand,
}

#[derive(Subcommand, Debug)]
pub(super) enum DevCommand {
    /// Syntax and semantic analysis tools
    Syntax(DevSyntaxArgs),
    /// Compile, test, and core library tools
    Build(DevBuildArgs),
    /// Dependency resolution and project graph tools
    Project(DevProjectArgs),
    /// Package registry tools
    Package(DevPackageArgs),
}

#[derive(clap::Args, Debug)]
pub(super) struct DevSyntaxArgs {
    #[command(subcommand)]
    pub(super) command: DevSyntaxCommand,
}

#[derive(Subcommand, Debug)]
pub(super) enum DevSyntaxCommand {
    /// Parse a Beskid file and output the AST representation
    Parse(ParseArgs),
    /// Generate an AST visualization tree from a Beskid file
    Tree(TreeArgs),
    /// Run semantic analysis and print diagnostics
    Analyze(AnalyzeArgs),
    /// Emit API documentation for a resolved source file
    Doc(DocArgs),
    /// Pretty-print Beskid sources using the canonical formatter
    Format(FormatArgs),
    /// Lower a Beskid file into CLIF and print the resulting IR
    Clif(ClifArgs),
}

#[derive(clap::Args, Debug)]
pub(super) struct DevBuildArgs {
    #[command(subcommand)]
    pub(super) command: DevBuildCommand,
}

#[derive(Subcommand, Debug)]
pub(super) enum DevBuildCommand {
    /// AOT-compile and link a Beskid file into output artifacts
    Compile(BuildArgs),
    /// Discover and run Beskid `test` items
    Test(TestArgs),
    /// Materialize the checked-in Beskid corelib project template
    Corelib(CorelibArgs),
}

#[derive(clap::Args, Debug)]
pub(super) struct DevProjectArgs {
    #[command(subcommand)]
    pub(super) command: DevProjectCommand,
}

#[derive(Subcommand, Debug)]
pub(super) enum DevProjectCommand {
    /// Resolve and materialize project dependencies
    Fetch(FetchArgs),
    /// Synchronize Project.lock for a project
    Lock(LockArgs),
    /// Update dependency resolution and materialized workspace
    Update(UpdateArgs),
    /// Visualize project/workspace graphs
    Graph(GraphArgs),
}

#[derive(clap::Args, Debug)]
pub(super) struct DevPackageArgs {
    #[command(subcommand)]
    pub(super) command: DevPackageCommand,
}

#[derive(Subcommand, Debug)]
pub(super) enum DevPackageCommand {
    /// Package-manager operations backed by the pckg registry service
    Registry(PckgArgs),
}
