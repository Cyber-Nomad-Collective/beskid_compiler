//! One target-free dependency resolution authority for preparation and mutations.
#![allow(non_snake_case)]

use super::lockfile::{preflight_existing_lock_for_plan, validate_existing_lock_graph};
use super::prepare::{installed_corelib_lock_root, portable_entry_for_dependency};
use super::registry::{ResolvedRegistryDependency, resolve_registry_dependency};
use super::{ProjectLockDependencyEntry, ProjectLockSource, WorkspacePrepareOptions};
use crate::projects::{
    DependencySource, ProjectError, ProjectWorkspacePlan, ResolvedDependencyProject, discover_workspace_resolution_rules,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RefreshScope {
    #[default]
    None,
    Selected(BTreeSet<String>),
    All,
}
impl RefreshScope {
    pub(crate) fn Includes(&self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::Selected(names) => names.contains(name),
            Self::All => true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionPolicy {
    pub locked: bool,
    pub offline: bool,
    pub refresh: RefreshScope,
}

pub(super) struct ResolvedWorkspaceDependencies {
    pub entries: Vec<ProjectLockDependencyEntry>,
    pub registry: Vec<ResolvedRegistryDependency>,
}

pub(super) fn ResolveWorkspaceDependencies(
    plan: &ProjectWorkspacePlan,
    policy: &ResolutionPolicy,
    allow_graph_change: bool,
) -> Result<ResolvedWorkspaceDependencies, ProjectError> {
    if policy.locked && (allow_graph_change || policy.refresh != RefreshScope::None) {
        return Err(ProjectError::Validation("locked resolution forbids dependency mutation".into()));
    }
    let options = WorkspacePrepareOptions {
        offline: policy.offline,
        locked: policy.locked,
        frozen: false,
        refresh_lock: allow_graph_change || policy.refresh == RefreshScope::All,
    };
    let existing = preflight_existing_lock_for_plan(plan, options)?;
    let corelib = installed_corelib_lock_root();
    RejectAmbiguousDependencyNames(&plan.dependency_projects, corelib.as_deref())?;
    let mut entries = Vec::new();
    let mut destinations = HashSet::new();
    for dependency in &plan.dependency_projects {
        let entry = portable_entry_for_dependency(plan, dependency, corelib.as_deref())?;
        if !destinations.insert(entry.materialized_root.clone()) {
            return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
        }
        entries.push(entry);
    }
    let registry_notes = plan
        .unresolved_dependencies
        .iter()
        .filter(|dependency| dependency.source == DependencySource::Registry)
        .collect::<Vec<_>>();
    validate_existing_lock_graph(
        existing.as_ref(),
        &entries,
        &registry_notes.iter().map(|note| note.dependency_name.as_str()).collect::<Vec<_>>(),
        options.refresh_lock,
    )?;
    let rules = discover_workspace_resolution_rules(&plan.manifest_path)?;
    let mut registry = Vec::new();
    for note in registry_notes {
        let refresh = policy.refresh.Includes(&note.dependency_name);
        let pin = existing.as_ref().and_then(|lock| {
            lock.dependencies
                .iter()
                .find(|entry| entry.source == ProjectLockSource::Registry && entry.name == note.dependency_name)
        });
        let unpinned_in_existing_lock = existing.is_some() && pin.is_none() && !allow_graph_change && !refresh;
        if unpinned_in_existing_lock && (policy.locked || policy.offline) {
            return Err(ProjectError::Validation(format!(
                "registry dependency `{}` has no lock pin; run `beskid update {}`",
                note.dependency_name, note.dependency_name
            )));
        }
        let Some(resolved) = resolve_registry_dependency(
            note,
            rules.as_ref(),
            pin,
            refresh,
            &plan.project_root,
            policy.offline,
            !allow_graph_change,
        )?
        else {
            continue;
        };
        if unpinned_in_existing_lock {
            return Err(ProjectError::Validation(format!(
                "registry dependency `{}` is now available but has no lock pin; run `beskid update {}`",
                note.dependency_name, note.dependency_name
            )));
        }
        resolved.validate_existing_lock_entry(if refresh { None } else { pin })?;
        if !destinations.insert(resolved.materialized_relative.clone()) {
            return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
        }
        registry.push(resolved);
    }
    Ok(ResolvedWorkspaceDependencies { entries, registry })
}

/// One lock entry exists per dependency name, so one name must identify one package.
///
/// The project graph already unifies every edge that reaches the same manifest into one node
/// (one package identity, as Cargo keeps one `Cargo.lock` entry per package id). Two resolved
/// dependency projects that share a name therefore live in different package roots: they are
/// different packages, and the lock cannot record both. Fail closed and name both roots.
fn RejectAmbiguousDependencyNames(
    dependencies: &[ResolvedDependencyProject],
    corelibRoot: Option<&Path>,
) -> Result<(), ProjectError> {
    let canonicalCorelib = corelibRoot.and_then(|root| root.canonicalize().ok());
    let mut seen: HashMap<&str, (&ResolvedDependencyProject, PathBuf)> = HashMap::new();
    for dependency in dependencies {
        let root = dependency.project_root.canonicalize().unwrap_or_else(|_| dependency.project_root.clone());
        let Some((first, firstRoot)) = seen.get(dependency.dependency_name.as_str()) else {
            seen.insert(dependency.dependency_name.as_str(), (dependency, root));
            continue;
        };
        if *firstRoot == root {
            continue;
        }
        let inCorelib = |path: &Path| canonicalCorelib.as_deref().is_some_and(|corelib| path.starts_with(corelib));
        let hint = if inCorelib(firstRoot) != inCorelib(&root) {
            "; one copy comes from the installed Corelib closure that is attached implicitly as `Core`. \
             Declare the Corelib aggregate that owns the explicit path (for example \
             `dependency \"corelib\" { source = path path = \"<workspace>/beskid_corelib\" }`) \
             or remove the explicit path dependency"
        } else {
            "; point both dependency declarations at the same package or rename one dependency label"
        };
        return Err(ProjectError::Validation(format!(
            "dependency `{}` resolves to two different packages: {} and {}{hint}",
            dependency.dependency_name,
            first.manifest_path.display(),
            dependency.manifest_path.display(),
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::RejectAmbiguousDependencyNames;
    use crate::projects::{ProjectError, ResolvedDependencyProject};
    use std::fs;
    use std::path::Path;

    fn dependency(name: &str, root: &Path) -> ResolvedDependencyProject {
        fs::create_dir_all(root.join("src")).unwrap();
        ResolvedDependencyProject {
            dependency_name: name.to_string(),
            manifest_path: root.join(format!("{name}.bproj")),
            project_root: root.to_path_buf(),
            project_name: name.to_string(),
            source_root: root.join("src"),
        }
    }

    #[test]
    fn same_package_reached_twice_is_one_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let foundation = workspace.path().join("packages/foundation");
        let direct = dependency("corelib_foundation", &foundation);
        let viaAlias = dependency("corelib_foundation", &workspace.path().join("packages/../packages/foundation"));
        RejectAmbiguousDependencyNames(&[direct, viaAlias], None).unwrap();
    }

    #[test]
    fn two_packages_claiming_one_name_fail_closed_with_both_roots() {
        let checkout = tempfile::tempdir().unwrap();
        let installed = tempfile::tempdir().unwrap();
        let explicit = dependency("corelib_foundation", &checkout.path().join("packages/foundation"));
        let implicit = dependency("corelib_foundation", &installed.path().join("packages/foundation"));
        let error = RejectAmbiguousDependencyNames(&[explicit.clone(), implicit.clone()], Some(installed.path()))
            .unwrap_err();
        let ProjectError::Validation(message) = error else { panic!("expected a validation error") };
        assert!(message.contains("`corelib_foundation` resolves to two different packages"), "{message}");
        assert!(message.contains(&explicit.manifest_path.display().to_string()), "{message}");
        assert!(message.contains(&implicit.manifest_path.display().to_string()), "{message}");
        assert!(message.contains("implicitly as `Core`"), "{message}");
    }
}
