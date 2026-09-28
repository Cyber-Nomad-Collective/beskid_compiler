use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::archive::extract_zip_to_dir;
use super::filesystem::materialized_dependency_id;
use super::lockfile::{
    PortableLockPath, PortableLockPathBaseKind, ProjectLockDependencyEntry, ProjectLockSource,
};
use crate::projects::error::ProjectError;
use crate::projects::graph::WorkspaceResolutionRules;
use crate::projects::model::{MaterializedDependencyProject, UnresolvedDependencyNote};

pub(super) struct ResolvedRegistryDependency {
    pub(super) dependency_name: String,
    pub(super) materialized_relative: String,
    materialized_root: PathBuf,
    artifact: Vec<u8>,
    selected_version: String,
    artifact_digest: String,
    registry_identity: String,
}

pub(super) fn resolve_registry_dependency(
    unresolved: &UnresolvedDependencyNote,
    workspace_rules: Option<&WorkspaceResolutionRules>,
    pinned: Option<&ProjectLockDependencyEntry>,
    refresh: bool,
    project_root: &Path,
) -> Result<Option<ResolvedRegistryDependency>, ProjectError> {
    let (registry_alias, requested_version) = parse_registry_descriptor(&unresolved.descriptor);
    if let Some(alias) = registry_alias.as_deref()
        && !workspace_rules.is_some_and(|rules| rules.has_registry_alias(alias))
    {
        return Err(ProjectError::Validation(format!(
            "registry dependency `{}` references an unknown workspace registry alias",
            unresolved.dependency_name
        )));
    }
    let registry_identity = registry_alias.as_deref().unwrap_or("default").to_ascii_lowercase();
    if let Some(pin) = pinned
        && !refresh
        && (pin.source != ProjectLockSource::Registry
            || pin.name != unresolved.dependency_name
            || pin.registry.as_deref() != Some(registry_identity.as_str()))
    {
        return Err(ProjectError::Validation(format!(
            "registry pin for `{}` does not match the current dependency; run `beskid update`",
            unresolved.dependency_name
        )));
    }
    let base_url = resolve_registry_base_url(workspace_rules, registry_alias.as_deref());
    let selected_version = if !refresh {
        pinned.and_then(|entry| entry.resolved_version.clone())
    } else {
        None
    };
    let selected_version = match selected_version {
        Some(version) => version,
        None => match select_registry_version(&base_url, &unresolved.dependency_name, requested_version.as_deref()) {
            Ok(version) => version,
            Err(_) if pinned.is_none() && !refresh => return Ok(None),
            Err(error) => return Err(error),
        },
    };

    let download_url = registry_url(&base_url, &["api", "packages", &unresolved.dependency_name, "versions", &selected_version, "download"])?;
    let artifact = match http_get_bytes(&download_url) {
        Ok(artifact) => artifact,
        Err(_) if pinned.is_none() && !refresh => return Ok(None),
        Err(error) => return Err(error),
    };
    let artifact_digest = format!("sha256:{:x}", Sha256::digest(&artifact));
    if let Some(pin) = pinned
        && !refresh
        && pin.artifact_digest.as_deref() != Some(artifact_digest.as_str())
    {
        return Err(ProjectError::Validation(format!(
            "registry artifact digest mismatch for `{}` at pinned version `{selected_version}`",
            unresolved.dependency_name
        )));
    }

    let identity = registry_materialization_identity(
        &registry_identity,
        &unresolved.dependency_name,
        &selected_version,
        &artifact_digest,
    );
    let materialized_name = materialized_dependency_id(
        &unresolved.dependency_name,
        ProjectLockSource::Registry,
        &identity,
    )?;
    let materialized_relative = format!("obj/beskid/deps/src/{materialized_name}");
    let materialized_root = PortableLockPath::parse(
        "materialized_root",
        &materialized_relative,
        PortableLockPathBaseKind::MaterializedRoot,
    )?
    .resolve(project_root)?;
    let owned_root = project_root.canonicalize().map_err(|_| {
        ProjectError::Validation("project root cannot be resolved".into())
    })?.join("obj/beskid/deps/src");
    if materialized_root.parent() != Some(owned_root.as_path()) {
        return Err(ProjectError::Validation("registry destination escapes its materialization root".into()));
    }
    Ok(Some(ResolvedRegistryDependency {
        dependency_name: unresolved.dependency_name.clone(), materialized_relative,
        materialized_root, artifact, selected_version, artifact_digest, registry_identity,
    }))
}

pub(super) fn materialize_registry_dependency(
    resolved: ResolvedRegistryDependency,
) -> Result<(ProjectLockDependencyEntry, MaterializedDependencyProject), ProjectError> {
    let ResolvedRegistryDependency {
        dependency_name, materialized_relative, materialized_root, artifact,
        selected_version, artifact_digest, registry_identity,
    } = resolved;
    fs::create_dir_all(&materialized_root)
        .map_err(|source| ProjectError::MaterializationCreateDir { path: materialized_root.clone(), source })?;
    extract_zip_to_dir(&artifact, &materialized_root)?;

    let manifest_path =
        crate::projects::discovery::discover_project_manifest_in_dir(&materialized_root)?.ok_or_else(|| {
            ProjectError::Validation(format!(
                "registry artifact for {}:{} missing a `.bproj` manifest",
                dependency_name, selected_version
            ))
        })?;

    let materialized_source_root = if materialized_root.join("src").is_dir() {
        materialized_root.join("src")
    } else if materialized_root.join("Src").is_dir() {
        materialized_root.join("Src")
    } else {
        materialized_root.clone()
    };

    let lock_entry = ProjectLockDependencyEntry {
        name: dependency_name.clone(),
        source: ProjectLockSource::Registry,
        manifest: portable_child_path(&materialized_root, &manifest_path)?,
        project: materialized_relative.clone(),
        source_root: portable_child_path(&materialized_root, &materialized_source_root)?,
        materialized_root: materialized_relative,
        resolved_version: Some(selected_version.clone()),
        artifact_digest: Some(artifact_digest),
        registry: Some(registry_identity),
    };

    let materialized_dependency = MaterializedDependencyProject {
        dependency_name: dependency_name.clone(),
        manifest_path,
        project_name: dependency_name,
        materialized_project_root: materialized_root,
        materialized_source_root,
    };

    Ok((lock_entry, materialized_dependency))
}

fn select_registry_version(base_url: &str, package: &str, requested_version: Option<&str>) -> Result<String, ProjectError> {
    let versions_url = registry_url(base_url, &["api", "packages", package, "versions"])?;
    let versions_json = http_get_text(&versions_url)?;
    let versions: Vec<Value> = serde_json::from_str(&versions_json)
        .map_err(|error| ProjectError::Validation(format!("registry version response is malformed: {error}")))?;
    let requested = requested_version.filter(|version| *version != "*");
    versions
        .iter()
        .find(|item| {
            !item.get("isYanked").and_then(Value::as_bool).unwrap_or(false)
                && requested.is_none_or(|version| item.get("version").and_then(Value::as_str) == Some(version))
        })
        .and_then(|item| item.get("version").and_then(Value::as_str))
        .map(str::to_owned)
        .ok_or_else(|| ProjectError::Validation(format!("registry package `{package}` has no available requested version")))
}

fn registry_url(base_url: &str, segments: &[&str]) -> Result<String, ProjectError> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|_| ProjectError::Validation("registry base URL is invalid".into()))?;
    url.path_segments_mut()
        .map_err(|_| ProjectError::Validation("registry base URL cannot have path segments".into()))?
        .pop_if_empty()
        .extend(segments);
    Ok(url.to_string())
}

fn registry_materialization_identity(alias: &str, package: &str, version: &str, digest: &str) -> String {
    let mut identity = String::new();
    for field in [alias, package, version, digest] {
        identity.push_str(&format!("{}:", field.len()));
        identity.push_str(field);
    }
    identity
}

fn portable_child_path(root: &Path, child: &Path) -> Result<String, ProjectError> {
    let relative = child.strip_prefix(root).map_err(|_| {
        ProjectError::Validation("registry artifact path escapes its materialized project".into())
    })?;
    let parts = relative
        .components()
        .map(|part| part.as_os_str().to_str().map(str::to_owned).ok_or_else(|| {
            ProjectError::Validation("registry artifact path is not UTF-8".into())
        }))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if parts.is_empty() { ".".to_string() } else { parts.join("/") })
}

fn resolve_registry_base_url(
    workspace_rules: Option<&WorkspaceResolutionRules>,
    registry_alias: Option<&str>,
) -> String {
    if let Ok(url) = env::var("BESKID_PCKG_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
    }
    if let Some(rules) = workspace_rules
        && let Some(url) = rules.registry_base_url(registry_alias)
    {
        return url.trim_end_matches('/').to_string();
    }
    "https://pckg.beskid-lang.org".trim_end_matches('/').to_string()
}

fn parse_registry_descriptor(descriptor: &str) -> (Option<String>, Option<String>) {
    if let Some((left, right)) = descriptor.split_once('@') {
        if left.trim().is_empty() {
            return (None, Some(right.trim().to_string()));
        }
        return (Some(left.trim().to_string()), Some(right.trim().to_string()));
    }
    let trimmed = descriptor.trim();
    if trimmed.is_empty() { (None, None) } else { (None, Some(trimmed.to_string())) }
}

fn http_get_text(url: &str) -> Result<String, ProjectError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| ProjectError::Validation(format!("failed to build registry client: {err}")))?;
    let response = client
        .get(url)
        .send()
        .map_err(|_| ProjectError::Validation("registry request failed".into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!("registry request failed with status {status}")));
    }
    response
        .text()
        .map_err(|_| ProjectError::Validation("failed to read registry response".into()))
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>, ProjectError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| ProjectError::Validation(format!("failed to build registry client: {err}")))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|_| ProjectError::Validation("registry request failed".into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!("registry request failed with status {status}")));
    }
    let mut buffer = Vec::new();
    response
        .read_to_end(&mut buffer)
        .map_err(|_| ProjectError::Validation("failed to read registry response bytes".into()))?;
    Ok(buffer)
}
