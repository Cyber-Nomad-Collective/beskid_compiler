//! Single-process corelib matrix driver: run all Test targets sharing workspace resolution.

use anyhow::{Result, anyhow};
use beskid_analysis::projects::{TargetKind, load_manifest_from_path};
use std::path::PathBuf;

use crate::commands::test::{TestArgs, execute_single_target};

pub fn execute_all_targets(mut args: TestArgs) -> Result<()> {
    let manifest_path = resolve_manifest_path(&args)?;
    let manifest = load_manifest_from_path(&manifest_path)
        .map_err(|err| anyhow!("failed to load {}: {err}", manifest_path.display()))?;

    let test_targets: Vec<String> = manifest
        .targets
        .iter()
        .filter(|target| target.kind == TargetKind::Test)
        .map(|target| target.name.clone())
        .collect();

    if test_targets.is_empty() {
        return Err(anyhow!("no Test targets in {}", manifest_path.display()));
    }

    let mut failures = Vec::new();
    for target in test_targets {
        args.project.target = Some(target.clone());
        if let Err(error) = execute_single_target(args.clone()) {
            eprintln!("[test] target `{target}` failed: {error}");
            failures.push(target);
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "matrix run failed for {} target(s): {}",
            failures.len(),
            failures.join(", ")
        ))
    }
}

fn resolve_manifest_path(args: &TestArgs) -> Result<PathBuf> {
    if let Some(project) = args.project.project.as_ref() {
        if project.extension().and_then(|ext: &std::ffi::OsStr| ext.to_str()) == Some("proj") {
            return Ok(project.clone());
        }
        return Ok(project.join("Project.proj"));
    }
    Err(anyhow!("--all-targets requires --project"))
}
