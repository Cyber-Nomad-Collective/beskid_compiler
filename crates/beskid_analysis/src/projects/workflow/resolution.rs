//! One target-free dependency resolution authority for preparation and mutations.
#![allow(non_snake_case)]

use super::lockfile::{preflight_existing_lock_for_plan, validate_existing_lock_graph};
use super::prepare::{installed_corelib_lock_root, portable_entry_for_dependency};
use super::registry::{ResolvedRegistryDependency, resolve_registry_dependency};
use super::{ProjectLockDependencyEntry, ProjectLockSource, WorkspacePrepareOptions};
use crate::projects::{DependencySource, ProjectError, ProjectWorkspacePlan, discover_workspace_resolution_rules};
use std::collections::{BTreeSet, HashSet};

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
