//! Direct dependency intent commands over the shared analysis transaction.
#![allow(non_snake_case)]
use crate::machine_output::Records;
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
    /// Print schema-1 newline-delimited JSON records (`change`, then one `result`) on stdout
    #[arg(long)]
    pub json: bool,
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
        "add",
        args.policy,
        DependencyMutation::Add(DependencyIntent { name: name.into(), source }),
        RefreshScope::None,
        false,
    )
}
pub fn Remove(args: RemoveArgs) -> Result<()> {
    Execute("remove", args.policy, DependencyMutation::Remove(args.package), RefreshScope::None, false)
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
        "update",
        args.policy,
        DependencyMutation::Update { package: args.package, version: args.version, all: args.all },
        refresh,
        args.dry_run,
    )
}
fn Execute(
    command: &'static str,
    args: DependencyPolicyArgs,
    mutation: DependencyMutation,
    refresh: RefreshScope,
    dry_run: bool,
) -> Result<()> {
    if args.json {
        return Records::new(command).Run(|records| Apply(args, mutation, refresh, dry_run, Some(records)));
    }
    Apply(args, mutation, refresh, dry_run, None).map(|_| ())
}

fn Apply(
    args: DependencyPolicyArgs,
    mutation: DependencyMutation,
    refresh: RefreshScope,
    dry_run: bool,
    records: Option<&Records>,
) -> Result<serde_json::Value> {
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
    let report = if dry_run {
        if records.is_none() {
            println!("{}", plan.Report());
        }
        serde_json::to_value(plan.Report())?
    } else {
        let committed = CommitDependencyChange(plan)?;
        if records.is_none() {
            println!("{committed}");
        }
        serde_json::to_value(&committed)?
    };
    match records {
        Some(records) => ChangeRecords(records, report, dry_run),
        None => Ok(serde_json::Value::Null),
    }
}

/// One `change` record per dependency effect, then the `result` fields. Unversioned path and Git
/// sources keep null version fields and report their actual coordinates.
fn ChangeRecords(records: &Records, report: serde_json::Value, dry_run: bool) -> Result<serde_json::Value> {
    let serde_json::Value::Object(mut report) = report else {
        anyhow::bail!("dependency change report is not a JSON object");
    };
    let effects = match report.remove("effects") {
        Some(serde_json::Value::Array(effects)) => effects,
        _ => Vec::new(),
    };
    for effect in effects {
        let serde_json::Value::Object(mut effect) = effect else {
            anyhow::bail!("dependency effect is not a JSON object");
        };
        let action = match effect.remove("action") {
            Some(serde_json::Value::String(action)) => action,
            _ => anyhow::bail!("dependency effect has no action"),
        };
        records.Emit("change", &action, serde_json::Value::Object(effect))?;
    }
    Ok(serde_json::json!({
        "manifest": report.remove("path"),
        "operation": report.remove("operation"),
        "changed": report.remove("changed"),
        "dryRun": dry_run,
    }))
}
