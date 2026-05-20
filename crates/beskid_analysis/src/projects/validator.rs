use std::collections::HashSet;
use std::path::{Component, Path};

use crate::projects::error::ProjectError;
use crate::projects::model::{DependencySource, ProjectKind, ProjectManifest, WorkspaceManifest};

/// Closed capability names accepted in `project.mod.capabilities` (compiler-mod host bridge).
pub const MOD_CAPABILITY_NAMES: &[&str] = &[
    "emit_syntax",
    "read_project_sources",
    "query_semantic_snapshot",
    "extern_ffi",
    "rewrite_syntax",
];

const MOD_ARTIFACT_POLICIES: &[&str] = &["reuse", "rebuild", "clean_rebuild"];

pub fn validate_manifest(manifest: &ProjectManifest) -> Result<(), ProjectError> {
    if manifest.project.name.trim().is_empty() {
        return Err(ProjectError::Validation(
            "`project.name` is required".to_string(),
        ));
    }
    if manifest.project.version.trim().is_empty() {
        return Err(ProjectError::Validation(
            "`project.version` is required".to_string(),
        ));
    }
    if manifest.project.root.trim().is_empty() {
        return Err(ProjectError::Validation(
            "`project.root` cannot be empty".to_string(),
        ));
    }
    match manifest.project.kind {
        ProjectKind::Host if manifest.targets.is_empty() => {
            return Err(ProjectError::Validation(
                "at least one `target` block is required for host projects".to_string(),
            ));
        }
        ProjectKind::Mod if !manifest.targets.is_empty() => {
            return Err(ProjectError::meta_contract(
                "E1801",
                "`Mod` projects must not declare `target` blocks in v0.2 (compiler mods are discovered via dependencies and contracts)",
            ));
        }
        ProjectKind::Mod if manifest.project.mod_section.is_none() => {
            return Err(ProjectError::meta_contract(
                "E1802",
                "`type = Mod` requires a nested `mod { ... }` block under `project`",
            ));
        }
        _ => {}
    }

    let mut target_names = HashSet::new();
    for target in &manifest.targets {
        if !target_names.insert(target.name.clone()) {
            return Err(ProjectError::Validation(format!(
                "duplicate target label `{}`",
                target.name
            )));
        }
        validate_relative_entry_path(&target.entry)?;
    }

    if let Some(mod_section) = &manifest.project.mod_section {
        if let Some(n) = mod_section.max_generator_rounds
            && n == 0
        {
            return Err(ProjectError::meta_contract(
                "E1803",
                "`project.mod.maxGeneratorRounds` must be a positive integer",
            ));
        }
        if let Some(caps) = &mod_section.capabilities {
            for cap in caps {
                if !MOD_CAPABILITY_NAMES.iter().any(|known| *known == cap) {
                    return Err(ProjectError::meta_contract(
                        "E1804",
                        format!("unknown `project.mod.capabilities` entry `{cap}`"),
                    ));
                }
            }
        }
        if let Some(policy) = mod_section.artifact_policy.as_deref()
            && !MOD_ARTIFACT_POLICIES.contains(&policy)
        {
            return Err(ProjectError::meta_contract(
                "E1805",
                format!(
                    "unknown `project.mod.artifactPolicy` `{policy}` (expected reuse, rebuild, or clean_rebuild)"
                ),
            ));
        }
    }

    let mut dependency_names = HashSet::new();
    for dependency in &manifest.dependencies {
        if !dependency_names.insert(dependency.name.clone()) {
            return Err(ProjectError::Validation(format!(
                "duplicate dependency label `{}`",
                dependency.name
            )));
        }

        match dependency.source {
            DependencySource::Path => {
                if dependency.name.eq_ignore_ascii_case("Std") {
                    continue;
                }
                if dependency
                    .path
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(ProjectError::Validation(format!(
                        "dependency `{}` with source = path requires `path`",
                        dependency.name
                    )));
                }
            }
            DependencySource::Git => {
                if dependency
                    .url
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(ProjectError::Validation(format!(
                        "dependency `{}` with source = git requires `url`",
                        dependency.name
                    )));
                }
                if dependency
                    .rev
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(ProjectError::Validation(format!(
                        "dependency `{}` with source = git requires `rev`",
                        dependency.name
                    )));
                }
            }
            DependencySource::Registry => {
                if dependency
                    .version
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
                {
                    return Err(ProjectError::Validation(format!(
                        "dependency `{}` with source = registry requires `version`",
                        dependency.name
                    )));
                }
            }
        }
    }

    Ok(())
}

fn validate_relative_workspace_path(
    path_value: &str,
    field_name: &str,
) -> Result<(), ProjectError> {
    let path = Path::new(path_value);
    if path.is_absolute() {
        return Err(ProjectError::Validation(format!(
            "{field_name} must be relative: `{path_value}`"
        )));
    }

    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ProjectError::Validation(format!(
            "{field_name} cannot escape workspace root: `{path_value}`"
        )));
    }

    Ok(())
}

pub fn validate_workspace_manifest(manifest: &WorkspaceManifest) -> Result<(), ProjectError> {
    if manifest.workspace.name.trim().is_empty() {
        return Err(ProjectError::Validation(
            "`workspace.name` is required".to_string(),
        ));
    }
    let resolver = manifest.workspace.resolver.trim();
    if resolver.is_empty() {
        return Err(ProjectError::Validation(
            "`workspace.resolver` cannot be empty".to_string(),
        ));
    }
    if resolver != "v1" {
        return Err(ProjectError::Validation(format!(
            "unsupported workspace.resolver `{resolver}` (only `v1` is supported)"
        )));
    }

    let mut member_names = HashSet::new();
    for member in &manifest.members {
        if !member_names.insert(member.name.clone()) {
            return Err(ProjectError::Validation(format!(
                "duplicate member label `{}`",
                member.name
            )));
        }

        if member.path.trim().is_empty() {
            return Err(ProjectError::Validation(format!(
                "member `{}` requires non-empty `path`",
                member.name
            )));
        }

        validate_relative_workspace_path(&member.path, "member path")?;
    }

    let mut override_names = HashSet::new();
    for dependency_override in &manifest.overrides {
        if !override_names.insert(dependency_override.dependency.clone()) {
            return Err(ProjectError::Validation(format!(
                "duplicate override label `{}`",
                dependency_override.dependency
            )));
        }

        if dependency_override.version.trim().is_empty() {
            return Err(ProjectError::Validation(format!(
                "override `{}` requires non-empty `version`",
                dependency_override.dependency
            )));
        }
    }

    let mut registry_names = HashSet::new();
    for registry in &manifest.registries {
        if !registry_names.insert(registry.name.clone()) {
            return Err(ProjectError::Validation(format!(
                "duplicate registry label `{}`",
                registry.name
            )));
        }

        if registry.url.trim().is_empty() {
            return Err(ProjectError::Validation(format!(
                "registry `{}` requires non-empty `url`",
                registry.name
            )));
        }
    }

    Ok(())
}

fn validate_relative_entry_path(entry: &str) -> Result<(), ProjectError> {
    let path = Path::new(entry);
    if path.is_absolute() {
        return Err(ProjectError::Validation(format!(
            "target entry path must be relative: `{entry}`"
        )));
    }

    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ProjectError::Validation(format!(
            "target entry path cannot escape source root: `{entry}`"
        )));
    }

    Ok(())
}
