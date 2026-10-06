//! Direct dependency intent commands over the shared analysis transaction.
#![allow(non_snake_case)]
use crate::project_args::LockfilePolicyArgs;
use anyhow::Result;
use beskid_analysis::projects::dependency_edit::{
    CommitDependencyChange, DependencyIntent, DependencyIntentSource, DependencyMutation, PlanDependencyChange,
    SelectDependencyProject,
};
use beskid_analysis::projects::workflow::{RefreshScope, ResolutionPolicy};
use clap::Args;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct DependencyPolicyArgs {
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub lockfile: LockfilePolicyArgs,
}
#[derive(Args, Debug)]
pub struct AddArgs {
    /// Package name, optionally @exact-version.
    pub package: String,
    /// Local project dependency path, relative to the selected project directory.
    #[arg(long)]
    pub path: Option<PathBuf>,
    #[command(flatten)]
    pub policy: DependencyPolicyArgs,
}
#[derive(Args, Debug)]
pub struct RemoveArgs {
    pub package: String,
    #[command(flatten)]
    pub policy: DependencyPolicyArgs,
}
#[derive(Args, Debug)]
pub struct UpdateArgs {
    #[arg(required_unless_present = "all", conflicts_with = "all")]
    pub package: Option<String>,
    #[arg(long, conflicts_with_all = ["package", "version"])]
    pub all: bool,
    #[arg(long, requires = "package")]
    pub version: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub policy: DependencyPolicyArgs,
}

pub fn Add(args: AddArgs) -> Result<()> {
    let (name, version) =
        args.package.split_once('@').map_or((args.package.as_str(), None), |(name, version)| (name, Some(version)));
    if name.trim().is_empty() || version == Some("") {
        anyhow::bail!("add: expected package[@exact-version]; run `beskid add --help`");
    }
    let source = if let Some(path) = args.path {
        if version.is_some() {
            anyhow::bail!("add: --path conflicts with @version");
        }
        DependencyIntentSource::Path(path)
    } else {
        if let Some(version) = version {
            semver::Version::parse(version)
                .map_err(|_| anyhow::anyhow!("add: version must be exact SemVer; ranges and Git are unsupported"))?;
        }
        // Empty requested version delegates bare stable selection to the sole resolver.
        DependencyIntentSource::Registry { registry: None, version: version.unwrap_or("").into() }
    };
    Execute(
        args.policy,
        DependencyMutation::Add(DependencyIntent { name: name.into(), source }),
        RefreshScope::None,
        false,
    )
}
pub fn Remove(args: RemoveArgs) -> Result<()> {
    Execute(args.policy, DependencyMutation::Remove(args.package), RefreshScope::None, false)
}
pub fn Update(args: UpdateArgs) -> Result<()> {
    if let Some(version) = &args.version {
        semver::Version::parse(version)?;
    }
    let refresh = if args.all {
        RefreshScope::All
    } else {
        RefreshScope::Selected(BTreeSet::from([args.package.clone().expect("Clap requires package or --all")]))
    };
    Execute(
        args.policy,
        DependencyMutation::Update { package: args.package, version: args.version, all: args.all },
        refresh,
        args.dry_run,
    )
}
fn Execute(
    args: DependencyPolicyArgs,
    mutation: DependencyMutation,
    refresh: RefreshScope,
    dry_run: bool,
) -> Result<()> {
    let manifest = SelectDependencyProject(&std::env::current_dir()?, args.project.as_deref())?;
    if !dry_run {
        beskid_analysis::projects::dependency_transaction::RecoverDependencyPair(&manifest)?;
    }
    let policy = ResolutionPolicy {
        locked: args.lockfile.locked || args.lockfile.frozen,
        offline: args.lockfile.offline || args.lockfile.frozen,
        refresh,
    };
    let plan = PlanDependencyChange(&manifest, &mutation, &policy)?;
    if dry_run {
        println!("{}", plan.Report());
        return Ok(());
    }
    let report = CommitDependencyChange(plan)?;
    println!("{report}");
    Ok(())
}
