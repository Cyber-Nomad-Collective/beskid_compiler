use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::projects::error::ProjectError;
use crate::projects::model::{
    CompilePlan, MaterializedDependencyProject, PreparedProjectWorkspace,
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
                ProjectError::Validation(
                    "lockfile dependency entry missing `manifest`".to_string(),
                )
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

