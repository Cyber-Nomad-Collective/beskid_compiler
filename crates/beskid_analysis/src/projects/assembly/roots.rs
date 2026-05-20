//! Effective (materialized-first) source roots for assembly and module-path checks.

use std::fs;
use std::path::{Path, PathBuf};

use crate::projects::workflow::ProjectLockDependencyEntry;
use crate::projects::{CompilePlan, PROJECT_LOCK_FILE_NAME, PreparedProjectWorkspace};

/// One searchable source root (host or named dependency).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootEntry {
    pub dependency_name: Option<String>,
    pub source_root: PathBuf,
}

/// Host + dependency source roots used for discovery and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveCompilationRoots {
    pub host: RootEntry,
    pub dependencies: Vec<RootEntry>,
}

/// Prefer materialized paths from `workspace`, else plan `source_root` paths.
pub fn effective_roots_from_plan_and_workspace(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
) -> EffectiveCompilationRoots {
    let (host_root, deps) = match workspace {
        Some(ws) => {
            let host = ws.materialized_source_root.clone();
            let deps = ws
                .materialized_dependencies
                .iter()
                .map(|dep| RootEntry {
                    dependency_name: Some(dep.dependency_name.clone()),
                    source_root: dep.materialized_source_root.clone(),
                })
                .collect();
            (host, deps)
        }
        None => {
            let host = plan.source_root.clone();
            let deps = plan
                .dependency_projects
                .iter()
                .map(|dep| RootEntry {
                    dependency_name: Some(dep.dependency_name.clone()),
                    source_root: dep.source_root.clone(),
                })
                .collect();
            (host, deps)
        }
    };

    EffectiveCompilationRoots {
        host: RootEntry {
            dependency_name: None,
            source_root: host_root,
        },
        dependencies: deps,
    }
}

/// Replay materialized roots from an on-disk `Project.lock` when no prepared workspace is available (LSP).
pub fn effective_roots_from_lockfile(
    plan: &CompilePlan,
    lockfile_path: &Path,
) -> EffectiveCompilationRoots {
    let mut base = effective_roots_from_plan_and_workspace(plan, None);
    let Ok(text) = fs::read_to_string(lockfile_path) else {
        return base;
    };

    let entries = parse_lockfile_dependency_lines(&text);
    if entries.is_empty() {
        return base;
    }

    for entry in entries {
        let materialized = PathBuf::from(entry.materialized_root());
        if !materialized.is_dir() {
            continue;
        }
        let project = PathBuf::from(entry.project());
        let source_root = PathBuf::from(entry.source_root());
        let relative = source_root
            .strip_prefix(&project)
            .unwrap_or(Path::new("src"));
        let effective = materialized.join(relative);
        if let Some(dep) = base
            .dependencies
            .iter_mut()
            .find(|d| d.dependency_name.as_deref() == Some(entry.name()))
        {
            dep.source_root = if effective.is_dir() {
                effective
            } else {
                materialized
            };
        }
    }

    let root_materialized = plan.project_root.join("obj").join("beskid").join("root");
    if root_materialized.is_dir() {
        let segment = plan
            .source_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Src".to_string());
        let candidate = root_materialized.join(segment);
        if candidate.is_dir() {
            base.host.source_root = candidate;
        }
    }

    base
}

fn parse_lockfile_dependency_lines(text: &str) -> Vec<ProjectLockDependencyEntry> {
    text.lines()
        .filter(|line| !line.starts_with('#') && line.contains("name="))
        .filter_map(|line| ProjectLockDependencyEntry::parse_v1_line(line).ok())
        .collect()
}

/// Effective roots for a compile plan: workspace, else lockfile beside manifest, else plan paths.
pub fn effective_roots_for_plan(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
) -> EffectiveCompilationRoots {
    if workspace.is_some() {
        return effective_roots_from_plan_and_workspace(plan, workspace);
    }
    let lockfile = plan.manifest_path.with_file_name(PROJECT_LOCK_FILE_NAME);
    if lockfile.is_file() {
        effective_roots_from_lockfile(plan, &lockfile)
    } else {
        effective_roots_from_plan_and_workspace(plan, None)
    }
}

pub fn module_roots_from_effective(roots: &EffectiveCompilationRoots) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(1 + roots.dependencies.len());
    out.push(roots.host.source_root.clone());
    out.extend(
        roots
            .dependencies
            .iter()
            .map(|entry| entry.source_root.clone()),
    );
    out
}

/// Legacy helper: plan-only roots (no materialization). Prefer [`effective_roots_for_plan`].
pub fn module_roots_for_plan(plan: &CompilePlan) -> Vec<PathBuf> {
    module_roots_from_effective(&effective_roots_from_plan_and_workspace(plan, None))
}
