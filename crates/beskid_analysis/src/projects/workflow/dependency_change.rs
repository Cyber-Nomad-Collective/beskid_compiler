//! Plan exact dependency intent and commit the manifest/lock pair through the guarded journal.
#![allow(non_snake_case)]
use super::resolution::{ResolveWorkspaceDependencies, ResolvedWorkspaceDependencies};
use super::{ProjectLockfileV2, RefreshScope, ResolutionPolicy};
use crate::projects::dependency_edit::{DependencyIntentSource, DependencyMutation, EditManifest};
use crate::projects::dependency_transaction::CommitDependencyPair;
use crate::projects::graph::builder::{build_project_graph_from_manifest, discover_workspace_resolution_rules};
use crate::projects::{ProjectError, UnresolvedDependencyPolicy, parse_manifest, workspace_plan_from_graph};
use serde::Serialize;
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyChangeReport {
    path: PathBuf,
    operation: &'static str,
    changed: bool,
    effects: Vec<DependencyEffect>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DependencyEffect {
    package: String,
    source: String,
    from_version: Option<String>,
    to_version: Option<String>,
    action: &'static str,
    from_coordinate: Option<String>,
    to_coordinate: Option<String>,
}
impl fmt::Display for DependencyChangeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} {}: {}",
            self.operation,
            self.path.display(),
            if self.changed { "dependency changes" } else { "unchanged" }
        )?;
        for effect in &self.effects {
            let coordinate = match (&effect.from_coordinate, &effect.to_coordinate) {
                (Some(before), Some(after)) if before != after => format!("{before} -> {after}"),
                (_, Some(after)) => after.clone(),
                (Some(before), None) => before.clone(),
                (None, None) => unreachable!("a dependency effect retains at least one coordinate"),
            };
            writeln!(f, "  {} {}: {}", effect.action, effect.package, coordinate)?;
        }
        Ok(())
    }
}

fn DependencyCoordinate(dependency: &crate::projects::Dependency) -> String {
    use crate::projects::DependencySource;
    match dependency.source {
        DependencySource::Path => format!("path {}", dependency.path.as_deref().unwrap_or("<missing path>")),
        DependencySource::Git => format!(
            "git {} @ {}",
            dependency.url.as_deref().unwrap_or("<missing URL>"),
            dependency.rev.as_deref().unwrap_or("HEAD")
        ),
        DependencySource::Registry => format!(
            "registry {} @ {}",
            dependency.registry.as_deref().unwrap_or("default"),
            dependency.version.as_deref().unwrap_or("<missing version>")
        ),
    }
}

pub struct DependencyChangePlan {
    manifest: PathBuf,
    original: Vec<u8>,
    original_lock: Option<Vec<u8>>,
    replacement: Vec<u8>,
    replacement_lock: Option<Vec<u8>>,
    resolved: Option<ResolvedWorkspaceDependencies>,
    report: DependencyChangeReport,
}
impl DependencyChangePlan {
    pub fn Report(&self) -> &DependencyChangeReport {
        &self.report
    }
}

fn ReadOptional(path: &Path) -> Result<Option<Vec<u8>>, ProjectError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::read(path).map(Some).map_err(|error| ProjectError::Validation(error.to_string()))
        }
        Ok(_) => Err(ProjectError::Validation(format!("{} must be a regular file", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ProjectError::Validation(error.to_string())),
    }
}

pub fn PlanDependencyChange(
    manifest: &Path,
    mutation: &DependencyMutation,
    policy: &ResolutionPolicy,
) -> Result<DependencyChangePlan, ProjectError> {
    let original =
        ReadOptional(manifest)?.ok_or_else(|| ProjectError::Validation("project manifest is absent".into()))?;
    let manifest = manifest.canonicalize().map_err(|error| ProjectError::Validation(error.to_string()))?;
    let root = manifest.parent().unwrap();
    if root.join(".beskid/dependency-transaction/journal.json").exists() {
        return Err(ProjectError::Validation(
            "an interrupted dependency transaction needs recovery before planning".into(),
        ));
    }
    let text = std::str::from_utf8(&original).map_err(|_| ProjectError::Validation("manifest must be UTF-8".into()))?;
    let before = parse_manifest(text)?;
    let original_lock = ReadOptional(&root.join("Project.lock"))?;
    if original_lock.as_ref().is_some_and(|bytes| bytes.starts_with(b"# Project.lock v1"))
        && !matches!(mutation, DependencyMutation::Update { .. })
    {
        return Err(ProjectError::Validation(
            "Project.lock v1 requires `beskid dev project lock` or explicit update migration".into(),
        ));
    }
    let mut mutation = mutation.clone();
    if let DependencyMutation::Add(intent) = &mut mutation
        && let DependencyIntentSource::Registry { registry, version } = &mut intent.source
    {
        if version.is_empty() {
            if let Some(existing) = before.dependencies.iter().find(|dependency| dependency.name == intent.name) {
                if existing.source != crate::projects::DependencySource::Registry || existing.registry != *registry {
                    return Err(ProjectError::Validation("existing dependency has different source intent".into()));
                }
                *version = existing
                    .version
                    .clone()
                    .ok_or_else(|| ProjectError::Validation("existing registry intent is not exact".into()))?;
            } else {
                if policy.offline {
                    return Err(ProjectError::Validation("bare add requires online stable version selection".into()));
                }
                let rules = discover_workspace_resolution_rules(&manifest)?;
                *version = super::registry::select_stable_intent(&intent.name, registry.as_deref(), rules.as_ref())?;
            }
        }
        semver::Version::parse(version)
            .map_err(|_| ProjectError::Validation("registry intent requires exact SemVer".into()))?;
    }
    let replacement = EditManifest(text, &mutation)?.into_bytes();
    let changed_intent = replacement != original;
    if policy.locked && (changed_intent || policy.refresh != RefreshScope::None) {
        return Err(ProjectError::Validation("locked policy forbids dependency mutation".into()));
    }
    if !changed_intent
        && !policy.locked
        && policy.refresh == RefreshScope::None
        && matches!(mutation, DependencyMutation::Remove(_))
    {
        let report =
            DependencyChangeReport { path: manifest.clone(), operation: "remove", changed: false, effects: Vec::new() };
        return Ok(DependencyChangePlan {
            manifest,
            original,
            replacement,
            replacement_lock: original_lock.clone(),
            original_lock,
            resolved: None,
            report,
        });
    }
    let after = parse_manifest(std::str::from_utf8(&replacement).unwrap())?;
    let graph = build_project_graph_from_manifest(&manifest, after.clone())?;
    let workspace = workspace_plan_from_graph(&graph, UnresolvedDependencyPolicy::Error)?;
    let resolved = ResolveWorkspaceDependencies(&workspace, policy, changed_intent)?;
    let mut entries = resolved.entries.clone();
    for registry in &resolved.registry {
        entries.push(registry.lock_entry()?);
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    let replacement_lock = Some(ProjectLockfileV2::from_plan(&workspace, &entries)?.to_v2_content().into_bytes());
    let mut effects = Vec::new();
    for dependency in &after.dependencies {
        let old = before.dependencies.iter().find(|old| old.name == dependency.name);
        if old != Some(dependency) || policy.refresh.Includes(&dependency.name) {
            effects.push(DependencyEffect {
                package: dependency.name.clone(),
                source: format!("{:?}", dependency.source),
                from_version: old.and_then(|old| old.version.clone()),
                to_version: dependency.version.clone(),
                action: if old.is_none() {
                    "added"
                } else if old != Some(dependency) {
                    "changed"
                } else {
                    "refreshed"
                },
                from_coordinate: old.map(DependencyCoordinate),
                to_coordinate: Some(DependencyCoordinate(dependency)),
            });
        }
    }
    for dependency in &before.dependencies {
        if !after.dependencies.iter().any(|item| item.name == dependency.name) {
            effects.push(DependencyEffect {
                package: dependency.name.clone(),
                source: format!("{:?}", dependency.source),
                from_version: dependency.version.clone(),
                to_version: None,
                action: "removed",
                from_coordinate: Some(DependencyCoordinate(dependency)),
                to_coordinate: None,
            });
        }
    }
    let report = DependencyChangeReport {
        path: manifest.clone(),
        operation: match mutation {
            DependencyMutation::Add(_) => "add",
            DependencyMutation::Remove(_) => "remove",
            DependencyMutation::Update { .. } => "update",
        },
        changed: changed_intent || replacement_lock != original_lock,
        effects,
    };
    Ok(DependencyChangePlan {
        manifest,
        original,
        original_lock,
        replacement,
        replacement_lock,
        resolved: Some(resolved),
        report,
    })
}

pub fn CommitDependencyChange(plan: DependencyChangePlan) -> Result<DependencyChangeReport, ProjectError> {
    if ReadOptional(&plan.manifest)?.as_deref() != Some(plan.original.as_slice())
        || ReadOptional(&plan.manifest.with_file_name("Project.lock"))? != plan.original_lock
    {
        return Err(ProjectError::Validation("concurrent dependency edit invalidated the plan".into()));
    }
    if !plan.report.changed {
        return Ok(plan.report);
    }
    if let Some(resolved) = plan.resolved {
        for registry in resolved.registry {
            registry.cache_artifact(plan.manifest.parent().unwrap())?;
            let destination = plan.manifest.parent().unwrap().join("obj/beskid/deps/src");
            fs::create_dir_all(&destination).map_err(|error| ProjectError::Validation(error.to_string()))?;
            super::registry::materialize_registry_dependency(registry)?;
        }
    }
    CommitDependencyPair(
        &plan.manifest,
        &plan.original,
        plan.original_lock.as_deref(),
        &plan.replacement,
        plan.replacement_lock.as_deref().ok_or_else(|| ProjectError::Validation("resolved plan has no lock".into()))?,
    )?;
    Ok(plan.report)
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn source_coordinate_change_retains_both_sides_without_registry_versions() {
        let report = DependencyChangeReport {
            path: PathBuf::from("App.bproj"), operation: "update", changed: true,
            effects: vec![DependencyEffect {
                package: "Local".into(), source: "Path".into(),
                from_version: None, to_version: None, action: "changed",
                from_coordinate: Some("path ../Old".into()),
                to_coordinate: Some("path ../New".into()),
            }],
        };
        assert!(report.to_string().contains("changed Local: path ../Old -> path ../New"));
        assert!(!report.to_string().contains("absent"));
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["effects"][0]["fromCoordinate"], "path ../Old");
        assert_eq!(json["effects"][0]["toCoordinate"], "path ../New");
        assert!(json["effects"][0]["fromVersion"].is_null());
        assert!(json["effects"][0]["toVersion"].is_null());
    }
}
