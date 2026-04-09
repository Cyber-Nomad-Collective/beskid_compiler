use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read};
use std::path::Path;

use serde_json::Value;
use zip::ZipArchive;

use crate::projects::error::ProjectError;
use crate::projects::model::{
    CompilePlan, DependencySource, MaterializedDependencyProject, PreparedProjectWorkspace,
    UnresolvedDependencyNote,
};

pub const PROJECT_LOCK_FILE_NAME: &str = "Project.lock";
const PROJECT_LOCK_HEADER_V1: &str = "# Project.lock v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLockDependencyEntry {
    name: String,
    manifest: String,
    project: String,
    source_root: String,
    materialized_root: String,
    resolved_version: Option<String>,
    artifact_digest: Option<String>,
    registry: Option<String>,
}

impl ProjectLockDependencyEntry {
    pub fn to_v1_line(&self) -> String {
        let mut line = format!(
            "name={};manifest={};project={};source_root={};materialized_root={}",
            self.name, self.manifest, self.project, self.source_root, self.materialized_root
        );

        if let Some(version) = &self.resolved_version {
            line.push_str(";resolved_version=");
            line.push_str(version);
        }
        if let Some(digest) = &self.artifact_digest {
            line.push_str(";artifact_digest=");
            line.push_str(digest);
        }
        if let Some(registry) = &self.registry {
            line.push_str(";registry=");
            line.push_str(registry);
        }

        line
    }

    pub fn parse_v1_line(line: &str) -> Result<Self, ProjectError> {
        let mut name = None;
        let mut manifest = None;
        let mut project = None;
        let mut source_root = None;
        let mut materialized_root = None;
        let mut resolved_version = None;
        let mut artifact_digest = None;
        let mut registry = None;

        for part in line.split(';') {
            let (key, value) = part.split_once('=').ok_or_else(|| {
                ProjectError::Validation(format!("invalid lockfile dependency field `{part}`"))
            })?;
            match key {
                "name" => name = Some(value.to_string()),
                "manifest" => manifest = Some(value.to_string()),
                "project" => project = Some(value.to_string()),
                "source_root" => source_root = Some(value.to_string()),
                "materialized_root" => materialized_root = Some(value.to_string()),
                "resolved_version" => resolved_version = Some(value.to_string()),
                "artifact_digest" => artifact_digest = Some(value.to_string()),
                "registry" => registry = Some(value.to_string()),
                _ => {}
            }
        }

        Ok(Self {
            name: name.ok_or_else(|| {
                ProjectError::Validation("lockfile dependency entry missing `name`".to_string())
            })?,
            manifest: manifest.ok_or_else(|| {
                ProjectError::Validation("lockfile dependency entry missing `manifest`".to_string())
            })?,
            project: project.ok_or_else(|| {
                ProjectError::Validation("lockfile dependency entry missing `project`".to_string())
            })?,
            source_root: source_root.ok_or_else(|| {
                ProjectError::Validation(
                    "lockfile dependency entry missing `source_root`".to_string(),
                )
            })?,
            materialized_root: materialized_root.ok_or_else(|| {
                ProjectError::Validation(
                    "lockfile dependency entry missing `materialized_root`".to_string(),
                )
            })?,
            resolved_version,
            artifact_digest,
            registry,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectLockfileV1 {
    root_manifest: String,
    project_name: String,
    dependencies: Vec<ProjectLockDependencyEntry>,
}

impl ProjectLockfileV1 {
    fn from_plan(plan: &CompilePlan, entries: &[ProjectLockDependencyEntry]) -> Self {
        let mut dependencies = entries.to_vec();
        dependencies.sort_by_key(ProjectLockDependencyEntry::to_v1_line);
        Self {
            root_manifest: plan.manifest_path.display().to_string(),
            project_name: plan.project_name.clone(),
            dependencies,
        }
    }

    fn parse_v1(content: &str) -> Result<Self, ProjectError> {
        let mut lines = content.lines();
        let header = lines.next().unwrap_or_default();
        if header.trim() != PROJECT_LOCK_HEADER_V1 {
            return Err(ProjectError::Validation(
                "lockfile header must be `# Project.lock v1`".to_string(),
            ));
        }

        let mut root_manifest = None;
        let mut project_name = None;
        let mut dependencies = Vec::new();
        let mut in_dependencies = false;

        for raw in lines {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(value) = line.strip_prefix("root_manifest=") {
                root_manifest = Some(value.to_string());
                continue;
            }
            if let Some(value) = line.strip_prefix("project_name=") {
                project_name = Some(value.to_string());
                continue;
            }
            if line == "dependencies:" {
                in_dependencies = true;
                continue;
            }
            if in_dependencies {
                if let Some(entry) = line.strip_prefix("- ") {
                    dependencies.push(ProjectLockDependencyEntry::parse_v1_line(entry)?);
                    continue;
                }
                return Err(ProjectError::Validation(format!(
                    "invalid lockfile dependency line `{line}`"
                )));
            }

            return Err(ProjectError::Validation(format!(
                "invalid lockfile line `{line}`"
            )));
        }

        let mut parsed = Self {
            root_manifest: root_manifest.ok_or_else(|| {
                ProjectError::Validation("lockfile missing `root_manifest`".to_string())
            })?,
            project_name: project_name.ok_or_else(|| {
                ProjectError::Validation("lockfile missing `project_name`".to_string())
            })?,
            dependencies,
        };
        parsed
            .dependencies
            .sort_by_key(ProjectLockDependencyEntry::to_v1_line);
        Ok(parsed)
    }

    fn to_v1_content(&self) -> String {
        let mut content = String::new();
        content.push_str(PROJECT_LOCK_HEADER_V1);
        content.push('\n');
        content.push_str(&format!("root_manifest={}\n", self.root_manifest));
        content.push_str(&format!("project_name={}\n", self.project_name));
        content.push_str("dependencies:\n");

        let mut dependencies = self.dependencies.clone();
        dependencies.sort_by_key(ProjectLockDependencyEntry::to_v1_line);
        for entry in dependencies {
            content.push_str("- ");
            content.push_str(&entry.to_v1_line());
            content.push('\n');
        }

        content
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspacePrepareOptions {
    pub frozen: bool,
    pub locked: bool,
}

pub fn prepare_project_workspace(
    plan: &CompilePlan,
) -> Result<PreparedProjectWorkspace, ProjectError> {
    prepare_project_workspace_with_options(plan, WorkspacePrepareOptions::default())
}

pub fn prepare_project_workspace_with_options(
    plan: &CompilePlan,
    options: WorkspacePrepareOptions,
) -> Result<PreparedProjectWorkspace, ProjectError> {
    let deps_root = plan
        .project_root
        .join("obj")
        .join("beskid")
        .join("deps")
        .join("src");
    let root_materialized_project = plan.project_root.join("obj").join("beskid").join("root");
    fs::create_dir_all(&deps_root).map_err(|source| ProjectError::MaterializationCreateDir {
        path: deps_root.clone(),
        source,
    })?;

    let source_segment = plan
        .source_root
        .file_name()
        .map(|segment| segment.to_string_lossy().to_string())
        .unwrap_or_else(|| "Src".to_string());
    let materialized_source_root = root_materialized_project.join(source_segment);
    copy_directory_when_newer(&plan.source_root, &materialized_source_root)?;

    let mut lock_entries = Vec::with_capacity(plan.dependency_projects.len());
    let mut materialized_dependencies = Vec::with_capacity(plan.dependency_projects.len());

    for dependency in &plan.dependency_projects {
        let materialized_root = deps_root.join(materialized_dependency_id(
            &dependency.project_name,
            &dependency.manifest_path,
        ));
        copy_directory_when_newer(&dependency.project_root, &materialized_root)?;

        lock_entries.push(ProjectLockDependencyEntry {
            name: dependency.dependency_name.clone(),
            manifest: dependency.manifest_path.display().to_string(),
            project: dependency.project_root.display().to_string(),
            source_root: dependency.source_root.display().to_string(),
            materialized_root: materialized_root.display().to_string(),
            resolved_version: None,
            artifact_digest: None,
            registry: None,
        });

        let source_relative = dependency
            .source_root
            .strip_prefix(&dependency.project_root)
            .unwrap_or_else(|_| std::path::Path::new(""));
        materialized_dependencies.push(MaterializedDependencyProject {
            dependency_name: dependency.dependency_name.clone(),
            manifest_path: dependency.manifest_path.clone(),
            project_name: dependency.project_name.clone(),
            materialized_project_root: materialized_root.clone(),
            materialized_source_root: materialized_root.join(source_relative),
        });
    }

    for unresolved in plan
        .unresolved_dependencies
        .iter()
        .filter(|x| x.source == DependencySource::Registry)
    {
        if let Some((lock_entry, materialized_dependency)) =
            materialize_registry_dependency(unresolved, &deps_root)?
        {
            lock_entries.push(lock_entry);
            materialized_dependencies.push(materialized_dependency);
        }
    }

    lock_entries.sort_by_key(|entry| entry.to_v1_line());
    let lockfile_path = sync_project_lockfile(plan, &lock_entries, options)?;

    Ok(PreparedProjectWorkspace {
        lockfile_path,
        materialized_project_root: root_materialized_project,
        materialized_source_root,
        materialized_dependencies,
    })
}

fn sync_project_lockfile(
    plan: &CompilePlan,
    lock_entries: &[ProjectLockDependencyEntry],
    options: WorkspacePrepareOptions,
) -> Result<std::path::PathBuf, ProjectError> {
    let lock_path = plan.project_root.join(PROJECT_LOCK_FILE_NAME);
    let expected_lockfile = ProjectLockfileV1::from_plan(plan, lock_entries);
    let expected_content = expected_lockfile.to_v1_content();

    if options.locked && !lock_path.is_file() {
        return Err(ProjectError::LockfileRequired { path: lock_path });
    }

    if lock_path.is_file() {
        let existing =
            fs::read_to_string(&lock_path).map_err(|source| ProjectError::LockfileRead {
                path: lock_path.clone(),
                source,
            })?;
        let existing_matches = if existing == expected_content {
            true
        } else {
            ProjectLockfileV1::parse_v1(&existing)
                .map(|parsed| parsed == expected_lockfile)
                .unwrap_or(false)
        };

        if existing_matches {
            return Ok(lock_path);
        }

        if options.frozen {
            return Err(ProjectError::LockfileFrozenMode);
        }

        if options.locked {
            return Err(ProjectError::LockfileOutOfDate {
                project: plan.project_name.clone(),
            });
        }
    } else if options.frozen {
        return Err(ProjectError::LockfileFrozenMode);
    }

    fs::write(&lock_path, expected_content).map_err(|source| ProjectError::LockfileWrite {
        path: lock_path.clone(),
        source,
    })?;

    Ok(lock_path)
}

fn copy_directory_when_newer(source: &Path, destination: &Path) -> Result<(), ProjectError> {
    fs::create_dir_all(destination).map_err(|source| ProjectError::MaterializationCreateDir {
        path: destination.to_path_buf(),
        source,
    })?;

    for entry in fs::read_dir(source).map_err(|err| ProjectError::MaterializationReadDir {
        path: source.to_path_buf(),
        source: err,
    })? {
        let entry = entry.map_err(|err| ProjectError::MaterializationReadDir {
            path: source.to_path_buf(),
            source: err,
        })?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type =
            entry
                .file_type()
                .map_err(|source| ProjectError::MaterializationMetadata {
                    path: entry_path.clone(),
                    source,
                })?;

        if file_type.is_dir() {
            copy_directory_when_newer(&entry_path, &destination_path)?;
            continue;
        }

        if file_type.is_file() {
            copy_file_when_newer(&entry_path, &destination_path)?;
        }
    }

    Ok(())
}

fn copy_file_when_newer(source: &Path, destination: &Path) -> Result<(), ProjectError> {
    let should_copy = if destination.is_file() {
        let source_modified = fs::metadata(source)
            .and_then(|metadata| metadata.modified())
            .map_err(|err| ProjectError::MaterializationMetadata {
                path: source.to_path_buf(),
                source: err,
            })?;
        let destination_modified = fs::metadata(destination)
            .and_then(|metadata| metadata.modified())
            .map_err(|source| ProjectError::MaterializationMetadata {
                path: destination.to_path_buf(),
                source,
            })?;
        source_modified > destination_modified
    } else {
        true
    };

    if should_copy {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| {
                ProjectError::MaterializationCreateDir {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }
        fs::copy(source, destination).map_err(|err| ProjectError::MaterializationCopy {
            from: source.to_path_buf(),
            to: destination.to_path_buf(),
            source: err,
        })?;
    }

    Ok(())
}

fn materialized_dependency_id(project_name: &str, manifest_path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    manifest_path.to_string_lossy().hash(&mut hasher);
    let hash = hasher.finish();
    format!("{}-{hash:016x}", sanitize_segment(project_name))
}

fn sanitize_segment(value: &str) -> String {
    let mut result = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        "dependency".to_string()
    } else {
        result
    }
}

fn materialize_registry_dependency(
    unresolved: &UnresolvedDependencyNote,
    deps_root: &Path,
) -> Result<Option<(ProjectLockDependencyEntry, MaterializedDependencyProject)>, ProjectError> {
    let (registry_alias, requested_version) = parse_registry_descriptor(&unresolved.descriptor);
    let base_url = resolve_registry_base_url();
    let versions_url = format!(
        "{}/api/packages/{}/versions",
        base_url, unresolved.dependency_name
    );
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
            !item
                .get("isYanked")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && requested_version
                    .as_deref()
                    .is_none_or(|req| item.get("version").and_then(Value::as_str) == Some(req))
        })
        .or_else(|| {
            versions.iter().find(|item| {
                !item
                    .get("isYanked")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        })
        .ok_or_else(|| {
            ProjectError::Validation(format!(
                "registry package {} has no active versions",
                unresolved.dependency_name
            ))
        })?;

    let selected_version = selected
        .get("version")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let Some(selected_version) = selected_version else {
        return Ok(None);
    };

    let download_url = format!(
        "{}/api/packages/{}/versions/{}/download",
        base_url, unresolved.dependency_name, selected_version
    );
    let artifact = match http_get_bytes(&download_url) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let materialized_root = deps_root.join(format!(
        "{}-registry-{}",
        sanitize_segment(&unresolved.dependency_name),
        sanitize_segment(&selected_version)
    ));
    fs::create_dir_all(&materialized_root).map_err(|source| {
        ProjectError::MaterializationCreateDir {
            path: materialized_root.clone(),
            source,
        }
    })?;
    extract_zip_to_dir(&artifact, &materialized_root)?;

    let manifest_path = materialized_root.join("Project.proj");
    if !manifest_path.is_file() {
        return Err(ProjectError::Validation(format!(
            "registry artifact for {}:{} missing Project.proj",
            unresolved.dependency_name, selected_version
        )));
    }

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

fn resolve_registry_base_url() -> String {
    env::var("BESKID_PCKG_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8082".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn parse_registry_descriptor(descriptor: &str) -> (Option<String>, Option<String>) {
    if let Some((left, right)) = descriptor.split_once('@') {
        if left.trim().is_empty() {
            return (None, Some(right.trim().to_string()));
        }
        return (
            Some(left.trim().to_string()),
            Some(right.trim().to_string()),
        );
    }
    let trimmed = descriptor.trim();
    if trimmed.is_empty() {
        (None, None)
    } else {
        (None, Some(trimmed.to_string()))
    }
}

fn http_get_text(url: &str) -> Result<String, ProjectError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| {
            ProjectError::Validation(format!("failed to build registry client: {err}"))
        })?;
    let response = client.get(url).send().map_err(|err| {
        ProjectError::Validation(format!("registry request failed for {url}: {err}"))
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!(
            "registry request failed for {url} with status {status}"
        )));
    }
    response.text().map_err(|err| {
        ProjectError::Validation(format!(
            "failed to read registry response from {url}: {err}"
        ))
    })
}

fn http_get_bytes(url: &str) -> Result<Vec<u8>, ProjectError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|err| {
            ProjectError::Validation(format!("failed to build registry client: {err}"))
        })?;
    let mut response = client.get(url).send().map_err(|err| {
        ProjectError::Validation(format!("registry request failed for {url}: {err}"))
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!(
            "registry request failed for {url} with status {status}"
        )));
    }
    let mut buffer = Vec::new();
    response.read_to_end(&mut buffer).map_err(|err| {
        ProjectError::Validation(format!("failed to read bytes from {url}: {err}"))
    })?;
    Ok(buffer)
}

fn extract_zip_to_dir(bytes: &[u8], output_dir: &Path) -> Result<(), ProjectError> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)
        .map_err(|err| ProjectError::Validation(format!("invalid registry artifact ZIP: {err}")))?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|err| {
            ProjectError::Validation(format!("failed to read registry artifact entry: {err}"))
        })?;
        let Some(path) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let target = output_dir.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|source| {
                ProjectError::MaterializationCreateDir {
                    path: target,
                    source,
                }
            })?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| {
                ProjectError::MaterializationCreateDir {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }

        let mut file =
            fs::File::create(&target).map_err(|source| ProjectError::MaterializationCopy {
                from: output_dir.to_path_buf(),
                to: target.clone(),
                source,
            })?;
        std::io::copy(&mut entry, &mut file).map_err(|source| {
            ProjectError::MaterializationCopy {
                from: output_dir.to_path_buf(),
                to: target,
                source,
            }
        })?;
    }

    Ok(())
}
