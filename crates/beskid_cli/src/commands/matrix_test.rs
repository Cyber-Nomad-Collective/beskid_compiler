//! Single-process corelib matrix driver: run all Test / Lib targets sharing one workspace
//! resolution so corelib is compiled once and cached in the process-scoped Salsa database.

use anyhow::{Result, anyhow};
use beskid_analysis::projects::{TargetKind, load_manifest_from_path};
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

    let test_targets: Vec<String> = manifest
        .targets
        .iter()
        .filter(|target| target.kind == TargetKind::Test || target.kind == TargetKind::Lib)
        .map(|target| target.name.clone())
        .collect();

    if test_targets.is_empty() {
        return Err(anyhow!(
            "no Test or Lib targets in {}",
            manifest_path.display()
        ));
    }

    let mut failures = Vec::new();
    let mut passed = 0usize;
    let total = test_targets.len();

    for target in test_targets {
        eprint!("Running {target}... ");
        let _ = std::io::Write::flush(&mut std::io::stderr());

        args.project.target = Some(target.clone());
        let start = Instant::now();
        match execute_single_target(args.clone()) {
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
        Err(anyhow!(
            "matrix run failed for {failed}/{total} target(s)"
        ))
    }
}

fn resolve_manifest_path(args: &TestArgs) -> Result<PathBuf> {
    if let Some(project) = args.project.project.as_ref() {
        if project
            .extension()
            .and_then(|ext: &std::ffi::OsStr| ext.to_str())
            == Some("proj")
        {
            return Ok(project.clone());
        }
        return Ok(project.join("Project.proj"));
    }
    Err(anyhow!("--all-targets requires --project"))
}