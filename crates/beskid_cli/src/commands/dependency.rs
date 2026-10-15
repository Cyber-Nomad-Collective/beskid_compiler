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
    #[arg(value_parser = ParsePackageSelector)]
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
#[command(group = clap::ArgGroup::new("scope").args(["package", "all"]).required(true))]
pub struct UpdateArgs {
    /// Direct dependency to refresh or move; use --all to refresh every dependency.
    pub package: Option<String>,
    /// Refresh every dependency at its exact declared intent.
    #[arg(long, conflicts_with = "version")]
    pub all: bool,
    /// Move the selected dependency to this exact version.
    #[arg(long, requires = "package", value_parser = ParseExactVersion)]
    pub version: Option<String>,
    #[arg(long)]
    pub dry_run: bool,
    #[command(flatten)]
    pub policy: DependencyPolicyArgs,
}

/// `package[@exact-version]`; a range or other selector is a usage error (exit 2) before any work.
fn ParsePackageSelector(value: &str) -> std::result::Result<String, String> {
    let (name, version) = value.split_once('@').map_or((value, None), |(name, version)| (name, Some(version)));
    if name.trim().is_empty() || version == Some("") {
        return Err("expected package[@exact-version], for example `Numbers` or `Numbers@1.2.0`".into());
    }
    if let Some(version) = version {
        ParseExactVersion(version)?;
    }
    Ok(value.to_owned())
}

/// An exact SemVer version; ranges, wildcards and Git sources are unsupported in 0.6.
fn ParseExactVersion(value: &str) -> std::result::Result<String, String> {
    semver::Version::parse(value).map(|_| value.to_owned()).map_err(|_| {
        format!("`{value}` is not an exact SemVer version; version ranges and Git sources are unsupported, so name one exact version such as `1.2.0`")
    })
}

pub fn Add(args: AddArgs) -> Result<()> {
    let (name, version) =
        args.package.split_once('@').map_or((args.package.as_str(), None), |(name, version)| (name, Some(version)));
    let source = if let Some(path) = args.path {
        if version.is_some() {
            anyhow::bail!("add: --path conflicts with @version");
        }
        DependencyIntentSource::Path(path)
    } else {
        // Clap validated the exact selector; an empty requested version delegates bare stable
        // selection to the sole resolver.
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
/// sources keep null version and digest fields and report their actual coordinates; the result
/// carries the `sha256:` digest of the lock before and after the change (planned, for --dry-run).
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
        "previousLockDigest": report.remove("previousLockDigest"),
        "lockDigest": report.remove("lockDigest"),
        "dryRun": dry_run,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v06_change_result_carries_lock_digests_and_effects_become_records() {
        let report = serde_json::json!({
            "path": "/work/App/App.bproj", "operation": "update", "changed": true,
            "previousLockDigest": "sha256:aa", "lockDigest": "sha256:bb",
            "effects": [{
                "package": "Math", "source": "Registry", "action": "changed",
                "fromVersion": "1.0.0", "toVersion": "1.1.0",
                "fromCoordinate": "registry default @ 1.0.0", "toCoordinate": "registry default @ 1.1.0",
                "fromDigest": "sha256:01", "toDigest": "sha256:02",
            }],
        });
        let result = ChangeRecords(&Records::new("update"), report, true).unwrap();
        assert_eq!(result["previousLockDigest"], "sha256:aa");
        assert_eq!(result["lockDigest"], "sha256:bb");
        assert_eq!(result["operation"], "update");
        assert_eq!(result["dryRun"], true);
        assert!(result.get("effects").is_none(), "effects are emitted as change records, not result fields");
        assert!(ChangeRecords(&Records::new("update"), serde_json::json!({"effects": [{"package": "X"}]}), false).is_err());
    }
}

#[cfg(test)]
mod selector_tests {
    use crate::cli::app::ParseInvocation;

    #[test]
    fn dep06_01_range_and_git_selectors_are_usage_errors() {
        for args in [
            vec!["beskid", "add", "Numbers@^1.0"],
            vec!["beskid", "add", "Numbers@1.x"],
            vec!["beskid", "add", "Numbers@>=1.0.0"],
            vec!["beskid", "add", "Numbers@"],
            vec!["beskid", "add", "Numbers", "--git", "https://example.invalid/numbers.git"],
            vec!["beskid", "update", "Numbers", "--version", "^2.0"],
        ] {
            let error = ParseInvocation(args.clone()).err().unwrap_or_else(|| panic!("{args:?} parsed"));
            assert_eq!(error.exit_code(), 2, "{args:?}: {error}");
        }
        assert!(ParseInvocation(["beskid", "add", "Numbers@1.2.0"]).is_ok());
        assert!(ParseInvocation(["beskid", "add", "Numbers"]).is_ok());
    }

    #[test]
    fn dep06_03_update_scope_is_a_usage_error_naming_package_and_all() {
        for args in [vec!["beskid", "update"], vec!["beskid", "update", "Numbers", "--all"], vec!["beskid", "update", "--all", "--version", "1.0.0"]] {
            let error = ParseInvocation(args.clone()).err().unwrap_or_else(|| panic!("{args:?} parsed"));
            assert_eq!(error.exit_code(), 2, "{args:?}: {error}");
            assert!(error.to_string().contains("--all"), "{args:?} names --all: {error}");
        }
        assert!(ParseInvocation(["beskid", "update", "--all"]).is_ok());
        assert!(ParseInvocation(["beskid", "update", "Numbers", "--version", "2.0.0"]).is_ok());
    }
}
