use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use super::archive::extract_zip_to_dir;
use super::filesystem::sanitize_segment;
use super::lockfile::ProjectLockDependencyEntry;
use crate::projects::error::ProjectError;
use crate::projects::graph::WorkspaceResolutionRules;
use crate::projects::model::{MaterializedDependencyProject, UnresolvedDependencyNote};

pub(super) fn materialize_registry_dependency(
    unresolved: &UnresolvedDependencyNote,
    deps_root: &Path,
    workspace_rules: Option<&WorkspaceResolutionRules>,
) -> Result<Option<(ProjectLockDependencyEntry, MaterializedDependencyProject)>, ProjectError> {
    let (registry_alias, requested_version) = parse_registry_descriptor(&unresolved.descriptor);
    let base_url = resolve_registry_base_url(workspace_rules, registry_alias.as_deref());
    let versions_url = format!("{}/api/packages/{}/versions", base_url, unresolved.dependency_name);
    let versions_json = match http_get_text(&versions_url) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let versions: Vec<Value> = match serde_json::from_str(&versions_json) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if versions.is_empty() {
        return Ok(None);
    }

    let selected = versions
        .iter()
        .find(|item| {
            !item.get("isYanked").and_then(Value::as_bool).unwrap_or(false)
                && requested_version
                    .as_deref()
                    .is_none_or(|req| item.get("version").and_then(Value::as_str) == Some(req))
        })
        .or_else(|| versions.iter().find(|item| !item.get("isYanked").and_then(Value::as_bool).unwrap_or(false)))
        .ok_or_else(|| {
            ProjectError::Validation(format!("registry package {} has no active versions", unresolved.dependency_name))
        })?;

    let selected_version = selected.get("version").and_then(Value::as_str).map(ToOwned::to_owned);
    let Some(selected_version) = selected_version else {
        return Ok(None);
    };

    let download_url =
        format!("{}/api/packages/{}/versions/{}/download", base_url, unresolved.dependency_name, selected_version);
    let artifact = match http_get_bytes(&download_url) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let materialized_root = deps_root.join(format!(
        "{}-registry-{}",
        sanitize_segment(&unresolved.dependency_name),
        sanitize_segment(&selected_version)
    ));
    fs::create_dir_all(&materialized_root)
        .map_err(|source| ProjectError::MaterializationCreateDir { path: materialized_root.clone(), source })?;
    extract_zip_to_dir(&artifact, &materialized_root)?;

    let manifest_path =
        crate::projects::discovery::discover_project_manifest_in_dir(&materialized_root)?.ok_or_else(|| {
            ProjectError::Validation(format!(
                "registry artifact for {}:{} missing a `.bproj` manifest",
                unresolved.dependency_name, selected_version
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
        name: unresolved.dependency_name.clone(),
        manifest: manifest_path.display().to_string(),
        project: materialized_root.display().to_string(),
        source_root: materialized_source_root.display().to_string(),
        materialized_root: materialized_root.display().to_string(),
        resolved_version: Some(selected_version.clone()),
        artifact_digest: None,
        registry: registry_alias,
    };

    let materialized_dependency = MaterializedDependencyProject {
        dependency_name: unresolved.dependency_name.clone(),
        manifest_path,
        project_name: unresolved.dependency_name.clone(),
        materialized_project_root: materialized_root,
        materialized_source_root,
    };

    Ok(Some((lock_entry, materialized_dependency)))
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
        .map_err(|err| ProjectError::Validation(format!("registry request failed for {url}: {err}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!("registry request failed for {url} with status {status}")));
    }
    response
        .text()
        .map_err(|err| ProjectError::Validation(format!("failed to read registry response from {url}: {err}")))
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>, ProjectError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| ProjectError::Validation(format!("failed to build registry client: {err}")))?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|err| ProjectError::Validation(format!("registry request failed for {url}: {err}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!("registry request failed for {url} with status {status}")));
    }
    let mut buffer = Vec::new();
    response
        .read_to_end(&mut buffer)
        .map_err(|err| ProjectError::Validation(format!("failed to read bytes from {url}: {err}")))?;
    Ok(buffer)
}
