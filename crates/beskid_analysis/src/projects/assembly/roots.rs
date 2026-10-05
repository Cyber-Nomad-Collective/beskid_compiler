//! Effective (materialized-first) source roots for assembly and module-path checks.

use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use crate::projects::{
    CompilePlan, PROJECT_LOCK_FILE_NAME, PreparedProjectWorkspace, ProjectError, ProjectLockDependencyEntry,
    ProjectLockSource,
    workflow::{installed_corelib_lock_root, load_project_lock_dependencies_for_plan},
};

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

    EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root: host_root }, dependencies: deps }
}

/// Replay materialized roots from an on-disk `Project.lock` for legacy fallback callers.
/// LSP consumers use [`effective_roots_from_lockfile_checked`] to report invalid locks.
pub fn effective_roots_from_lockfile(plan: &CompilePlan, lockfile_path: &Path) -> EffectiveCompilationRoots {
    effective_roots_from_lockfile_checked(plan, lockfile_path)
        .unwrap_or_else(|_| effective_roots_from_plan_and_workspace(plan, None))
}

/// Checked replay for non-update tooling: a present invalid lock is an error, not an empty graph.
/// Missing materialization or an untrusted replay hint still falls back to declared source roots.
pub fn effective_roots_from_lockfile_checked(
    plan: &CompilePlan,
    lockfile_path: &Path,
) -> Result<EffectiveCompilationRoots, ProjectError> {
    let base = effective_roots_from_plan_and_workspace(plan, None);
    match fs::symlink_metadata(lockfile_path) {
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(base),
        Err(source) => return Err(ProjectError::LockfileRead { path: lockfile_path.to_path_buf(), source }),
        Ok(_) => {}
    }
    let entries = load_project_lock_dependencies_for_plan(lockfile_path, plan)?;
    let Some(trusted_dependencies_root) = plan.project_root.join("obj/beskid/deps/src").canonicalize().ok() else {
        return Ok(base);
    };
    let Some(replayed_dependencies) =
        replayed_dependency_roots(plan, lockfile_path, &entries, &trusted_dependencies_root)
    else {
        return Ok(base);
    };
    let mut roots = base;
    roots.dependencies = replayed_dependencies;

    let root_materialized = plan.project_root.join("obj").join("beskid").join("root");
    if root_materialized.is_dir() {
        let segment =
            plan.source_root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Src".to_string());
        let candidate = root_materialized.join(segment);
        if candidate.is_dir() {
            roots.host.source_root = candidate;
        }
    }

    Ok(roots)
}

fn replayed_dependency_roots(
    plan: &CompilePlan,
    lockfile_path: &Path,
    entries: &[ProjectLockDependencyEntry],
    trusted_dependencies_root: &Path,
) -> Option<Vec<RootEntry>> {
    let expected_names: HashSet<_> =
        plan.dependency_projects.iter().map(|dependency| dependency.dependency_name.as_str()).collect();
    let entry_names: HashSet<_> = entries.iter().map(ProjectLockDependencyEntry::name).collect();
    if expected_names != entry_names || entries.len() != expected_names.len() {
        return None;
    }

    let lock_root = lockfile_path.parent()?;
    let corelib_lock_root = installed_corelib_lock_root();
    let mut replayed = Vec::with_capacity(entries.len());
    for entry in entries {
        let dependency =
            plan.dependency_projects.iter().find(|dependency| dependency.dependency_name == entry.name())?;
        let materialized = resolve_lock_path(lock_root, Path::new(entry.materialized_root()));
        let materialized = materialized.canonicalize().ok()?;
        if !materialized.starts_with(trusted_dependencies_root) {
            return None;
        }

        let project_base = match entry.source() {
            ProjectLockSource::Path | ProjectLockSource::Registry => lock_root,
            ProjectLockSource::Corelib => corelib_lock_root.as_deref()?,
        };
        let project = project_base.join(entry.project());
        let manifest = project.join(entry.manifest());
        let source_root = project.join(entry.source_root());
        // A lockfile is only a replay hint. Its identity fields must still describe the
        // dependency selected by the current graph; a copied or stale lock cannot confer
        // compiler-owned service authority on another source tree.
        if manifest.canonicalize().ok()? != dependency.manifest_path.canonicalize().ok()?
            || project.canonicalize().ok()? != dependency.project_root.canonicalize().ok()?
            || source_root.canonicalize().ok()? != dependency.source_root.canonicalize().ok()?
        {
            return None;
        }
        let relative = source_root.strip_prefix(&project).ok()?;
        if relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
        {
            return None;
        }
        let effective = materialized.join(relative).canonicalize().ok()?;
        if !effective.starts_with(&materialized) {
            return None;
        }
        replayed.push(RootEntry { dependency_name: Some(dependency.dependency_name.clone()), source_root: effective });
    }
    Some(replayed)
}

fn resolve_lock_path(lock_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { lock_root.join(path) }
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
    out.extend(roots.dependencies.iter().map(|entry| entry.source_root.clone()));
    out
}
