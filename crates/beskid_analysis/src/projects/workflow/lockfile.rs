use std::fs;
use std::path::Path;

use crate::projects::error::ProjectError;
use crate::projects::model::CompilePlan;

pub const PROJECT_LOCK_FILE_NAME: &str = "Project.lock";
const PROJECT_LOCK_HEADER_V1: &str = "# Project.lock v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLockDependencyEntry {
    pub(super) name: String,
    pub(super) manifest: String,
    pub(super) project: String,
    pub(super) source_root: String,
    pub(super) materialized_root: String,
    pub(super) resolved_version: Option<String>,
    pub(super) artifact_digest: Option<String>,
    pub(super) registry: Option<String>,
}

impl ProjectLockDependencyEntry {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    pub fn source_root(&self) -> &str {
        &self.source_root
    }

    pub fn materialized_root(&self) -> &str {
        &self.materialized_root
    }

    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    pub fn resolved_version(&self) -> Option<&str> {
        self.resolved_version.as_deref()
    }

    pub fn registry(&self) -> Option<&str> {
        self.registry.as_deref()
    }

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
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| ProjectError::Validation(format!("invalid lockfile dependency field `{part}`")))?;
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
            name: name
                .ok_or_else(|| ProjectError::Validation("lockfile dependency entry missing `name`".to_string()))?,
            manifest: manifest
                .ok_or_else(|| ProjectError::Validation("lockfile dependency entry missing `manifest`".to_string()))?,
            project: project
                .ok_or_else(|| ProjectError::Validation("lockfile dependency entry missing `project`".to_string()))?,
            source_root: source_root.ok_or_else(|| {
                ProjectError::Validation("lockfile dependency entry missing `source_root`".to_string())
            })?,
            materialized_root: materialized_root.ok_or_else(|| {
                ProjectError::Validation("lockfile dependency entry missing `materialized_root`".to_string())
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
            return Err(ProjectError::Validation("lockfile header must be `# Project.lock v1`".to_string()));
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
                return Err(ProjectError::Validation(format!("invalid lockfile dependency line `{line}`")));
            }

            return Err(ProjectError::Validation(format!("invalid lockfile line `{line}`")));
        }

        let mut parsed = Self {
            root_manifest: root_manifest
                .ok_or_else(|| ProjectError::Validation("lockfile missing `root_manifest`".to_string()))?,
            project_name: project_name
                .ok_or_else(|| ProjectError::Validation("lockfile missing `project_name`".to_string()))?,
            dependencies,
        };
        parsed.dependencies.sort_by_key(ProjectLockDependencyEntry::to_v1_line);
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

/// Load dependency lines from `project_root/Project.lock` when the file exists.
pub fn load_project_lock_dependencies(project_root: &Path) -> Result<Vec<ProjectLockDependencyEntry>, ProjectError> {
    let lock_path = project_root.join(PROJECT_LOCK_FILE_NAME);
    if !lock_path.is_file() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&lock_path)
        .map_err(|e| ProjectError::Validation(format!("failed to read {}: {e}", lock_path.display())))?;
    Ok(ProjectLockfileV1::parse_v1(&content)?.dependencies)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspacePrepareOptions {
    pub frozen: bool,
    pub locked: bool,
}

pub(super) fn sync_project_lockfile(
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
        let existing = fs::read_to_string(&lock_path)
            .map_err(|source| ProjectError::LockfileRead { path: lock_path.clone(), source })?;
        let existing_matches = if existing == expected_content {
            true
        } else {
            ProjectLockfileV1::parse_v1(&existing).map(|parsed| parsed == expected_lockfile).unwrap_or(false)
        };

        if existing_matches {
            return Ok(lock_path);
        }

        if options.frozen {
            return Err(ProjectError::LockfileFrozenMode);
        }

        if options.locked {
            return Err(ProjectError::LockfileOutOfDate { project: plan.project_name.clone() });
        }
    } else if options.frozen {
        return Err(ProjectError::LockfileFrozenMode);
    }

    fs::write(&lock_path, expected_content)
        .map_err(|source| ProjectError::LockfileWrite { path: lock_path.clone(), source })?;

    Ok(lock_path)
}
