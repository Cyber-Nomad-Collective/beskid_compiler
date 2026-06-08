//! Single-process corelib matrix driver: run all Test / Lib targets sharing one workspace
//! resolution so corelib is compiled once and cached in the process-scoped Salsa database.

use anyhow::{Result, anyhow};
use beskid_analysis::projects::{TargetKind, load_manifest_from_path};
use beskid_engine::Engine;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

use crate::commands::test::{TestArgs, execute_single_target};

pub fn execute_all_targets(mut args: TestArgs) -> Result<()> {
    let manifest_path = resolve_manifest_path(&args)?;
    let manifest = load_manifest_from_path(&manifest_path)
        .map_err(|err| anyhow!("failed to load {}: {err}", manifest_path.display()))?;

    if let Some(parent) = manifest_path.parent() {
        beskid_queries::configure_db_for_project(parent);
    }

    let mut test_targets: Vec<String> = manifest
        .targets
        .iter()
        .filter(|target| target.kind == TargetKind::Test || target.kind == TargetKind::Lib)
        .map(|target| target.name.clone())
        .collect();
    test_targets = filter_targets_by_env(test_targets)?;

    if test_targets.is_empty() {
        return Err(anyhow!(
            "no Test or Lib targets in {}",
            manifest_path.display()
        ));
    }

    let mut failures = Vec::new();
    let mut passed = 0usize;
    let total = test_targets.len();
    let mut engine = Engine::with_link_profile(args.runtime_profile.into());

    for target in test_targets {
        eprint!("Running {target}... ");
        let _ = std::io::Write::flush(&mut std::io::stderr());

        args.project.target = Some(target.clone());
        let start = Instant::now();
        match execute_single_target(args.clone(), Some(&mut engine)) {
            Ok(()) => {
                let elapsed = start.elapsed();
                eprintln!("PASS ({elapsed:.1?})");
                passed += 1;
            }
            Err(error) => {
                let elapsed = start.elapsed();
                eprintln!("FAIL ({elapsed:.1?}): {error}");
                failures.push(target);
            }
        }
    }

    let failed = failures.len();
    eprintln!("\nmatrix: {passed}/{total} passed, {failed}/{total} failed");
    if !failures.is_empty() {
        eprintln!("failed targets: {}", failures.join(", "));
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("matrix run failed for {failed}/{total} target(s)"))
    }
}

fn filter_targets_by_env(targets: Vec<String>) -> Result<Vec<String>> {
    let raw = env::var("BESKID_CORELIB_TEST_TARGETS").unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(targets);
    }
    let wanted: std::collections::HashSet<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    if wanted.is_empty() {
        return Ok(targets);
    }
    let available: std::collections::HashSet<&str> = targets.iter().map(String::as_str).collect();
    let missing: Vec<String> = wanted
        .iter()
        .filter(|name| !available.contains(name.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(anyhow!(
            "BESKID_CORELIB_TEST_TARGETS unknown targets: {}",
            missing.join(", ")
        ));
    }
    Ok(targets
        .into_iter()
        .filter(|name| wanted.contains(name))
        .collect())
}

fn resolve_manifest_path(args: &TestArgs) -> Result<PathBuf> {
    if let Some(project) = args.project.project.as_ref() {
        if project
            .extension()
            .and_then(|ext: &std::ffi::OsStr| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bproj") || ext.eq_ignore_ascii_case("bws"))
        {
            return Ok(project.clone());
        }
        if let Some(manifest) =
            beskid_analysis::projects::discover_project_manifest_in_dir(project)
                .map_err(anyhow::Error::from)?
        {
            return Ok(manifest);
        }
        if let Some(workspace) =
            beskid_analysis::projects::discover_workspace_manifest_in_dir(project)
                .map_err(anyhow::Error::from)?
        {
            return Ok(workspace);
        }
        return Err(anyhow!(
            "no `.bproj` or `.bws` manifest found in {}",
            project.display()
        ));
    }
    Err(anyhow!("--all-targets requires --project"))
}
