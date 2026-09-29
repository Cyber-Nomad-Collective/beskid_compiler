use std::collections::HashSet;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path};

use tempfile::NamedTempFile;

use crate::projects::error::ProjectError;
use crate::projects::model::CompilePlan;

mod portable_path;

pub use portable_path::{PortableLockPath, PortableLockPathBaseKind};

pub const PROJECT_LOCK_FILE_NAME: &str = "Project.lock";
const PROJECT_LOCK_HEADER_V1: &str = "# Project.lock v1";
const PROJECT_LOCK_HEADER_V2: &str = "# Project.lock v2";

fn regular_lockfile_exists(lock_path: &Path) -> Result<bool, ProjectError> {
    match fs::symlink_metadata(lock_path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => {
            Err(ProjectError::Validation(format!("Project.lock must be a regular file at {}", lock_path.display())))
        }
        Err(source) if source.kind() == ErrorKind::NotFound => Ok(false),
        Err(source) => Err(ProjectError::LockfileRead { path: lock_path.to_path_buf(), source }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectLockSource {
    Path,
    Corelib,
    Registry,
}

impl ProjectLockSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Corelib => "corelib",
            Self::Registry => "registry",
        }
    }

    fn parse(value: &str) -> Result<Self, ProjectError> {
        match value {
            "path" => Ok(Self::Path),
            "corelib" => Ok(Self::Corelib),
            "registry" => Ok(Self::Registry),
            _ => Err(ProjectError::Validation("unknown lockfile source".into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLockDependencyEntry {
    pub(super) name: String,
    pub(super) source: ProjectLockSource,
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

    pub fn source(&self) -> ProjectLockSource {
        self.source
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
                "name" if name.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation("lockfile dependency duplicates `name`".to_string()));
                }
                "manifest" if manifest.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation("lockfile dependency duplicates `manifest`".to_string()));
                }
                "project" if project.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation("lockfile dependency duplicates `project`".to_string()));
                }
                "source_root" if source_root.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation("lockfile dependency duplicates `source_root`".to_string()));
                }
                "materialized_root" if materialized_root.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation(
                        "lockfile dependency duplicates `materialized_root`".to_string(),
                    ));
                }
                "resolved_version" if resolved_version.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation(
                        "lockfile dependency duplicates `resolved_version`".to_string(),
                    ));
                }
                "artifact_digest" if artifact_digest.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation(
                        "lockfile dependency duplicates `artifact_digest`".to_string(),
                    ));
                }
                "registry" if registry.replace(value.to_string()).is_some() => {
                    return Err(ProjectError::Validation("lockfile dependency duplicates `registry`".to_string()));
                }
                "name" | "manifest" | "project" | "source_root" | "materialized_root" | "resolved_version"
                | "artifact_digest" | "registry" => {}
                _ => {}
            }
        }

        Ok(Self {
            name: name
                .ok_or_else(|| ProjectError::Validation("lockfile dependency entry missing `name`".to_string()))?,
            source: if registry.is_some() || resolved_version.is_some() {
                ProjectLockSource::Registry
            } else {
                ProjectLockSource::Path
            },
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
pub struct ProjectLockfileV2 {
    root_manifest: String,
    project_name: String,
    pub(super) dependencies: Vec<ProjectLockDependencyEntry>,
}

impl ProjectLockfileV2 {
    fn from_plan(plan: &CompilePlan, entries: &[ProjectLockDependencyEntry]) -> Result<Self, ProjectError> {
        let root_manifest = plan
            .manifest_path
            .strip_prefix(&plan.project_root)
            .map_err(|_| ProjectError::Validation("root manifest escapes the lock directory".into()))?;
        let root_manifest = root_manifest
            .components()
            .map(|part| match part {
                Component::Normal(name) => name
                    .to_str()
                    .map(str::to_string)
                    .ok_or_else(|| ProjectError::Validation("root manifest path is not UTF-8".into())),
                _ => Err(ProjectError::Validation("root manifest path has an invalid component".into())),
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        PortableLockPath::parse("root_manifest", &root_manifest, PortableLockPathBaseKind::LockDirectory)?;
        if entries.iter().any(|entry| {
            entry.source == ProjectLockSource::Registry
                && (entry.registry.is_none() || entry.resolved_version.is_none() || entry.artifact_digest.is_none())
        }) {
            return Err(ProjectError::Validation("registry dependency lacks a complete v2 pin".into()));
        }
        let candidate = Self { root_manifest, project_name: plan.project_name.clone(), dependencies: entries.to_vec() };
        Self::parse_v2(&candidate.to_v2_content())
    }

    pub fn parse_v2(content: &str) -> Result<Self, ProjectError> {
        let body = content
            .strip_suffix('\n')
            .ok_or_else(|| ProjectError::Validation("v2 lockfile must end with LF".into()))?;
        let lines: Vec<&str> = body.split('\n').collect();
        if lines.len() < 4 || lines[0] != PROJECT_LOCK_HEADER_V2 {
            return Err(ProjectError::Validation("lockfile header must be exactly `# Project.lock v2`".into()));
        }
        let root_manifest = parse_v2_top_level(lines[1], "root_manifest")?;
        let root_manifest =
            PortableLockPath::parse("root_manifest", &root_manifest, PortableLockPathBaseKind::LockDirectory)?
                .as_str()
                .to_string();
        let project_name = parse_v2_top_level(lines[2], "project_name")?;
        if lines[3] != "dependencies:" {
            return Err(ProjectError::Validation("v2 lockfile missing `dependencies:`".into()));
        }

        let mut dependencies = Vec::new();
        let mut names = HashSet::new();
        let mut destinations = HashSet::new();
        for line in &lines[4..] {
            let entry = parse_v2_entry(line)?;
            if !names.insert(entry.name.clone()) {
                return Err(ProjectError::Validation("lockfile duplicates a dependency name".into()));
            }
            if !destinations.insert(entry.materialized_root.clone()) {
                return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
            }
            dependencies.push(entry);
        }
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self { root_manifest, project_name, dependencies })
    }

    pub fn to_v2_content(&self) -> String {
        let mut content = format!(
            "{PROJECT_LOCK_HEADER_V2}\nroot_manifest={}\nproject_name={}\ndependencies:\n",
            encode_v2_value(&self.root_manifest),
            encode_v2_value(&self.project_name)
        );
        let mut dependencies = self.dependencies.iter().collect::<Vec<_>>();
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));
        for entry in dependencies {
            content.push_str("- name=");
            content.push_str(&encode_v2_value(&entry.name));
            content.push_str(";source=");
            content.push_str(entry.source.as_str());
            for (key, value) in [
                ("project", &entry.project),
                ("manifest", &entry.manifest),
                ("source_root", &entry.source_root),
                ("materialized_root", &entry.materialized_root),
            ] {
                content.push(';');
                content.push_str(key);
                content.push('=');
                content.push_str(&encode_v2_value(value));
            }
            if entry.source == ProjectLockSource::Registry {
                for (key, value) in [
                    ("registry", entry.registry.as_ref().expect("validated registry alias")),
                    ("resolved_version", entry.resolved_version.as_ref().expect("validated registry version")),
                    ("artifact_digest", entry.artifact_digest.as_ref().expect("validated registry digest")),
                ] {
                    content.push(';');
                    content.push_str(key);
                    content.push('=');
                    content.push_str(&encode_v2_value(value));
                }
            }
            content.push('\n');
        }
        content
    }
}

fn parse_v2_top_level(line: &str, expected_key: &str) -> Result<String, ProjectError> {
    let (key, value) = line
        .split_once('=')
        .ok_or_else(|| ProjectError::Validation(format!("v2 lockfile missing `{expected_key}`")))?;
    if key != expected_key {
        return Err(ProjectError::Validation(format!("v2 lockfile expected `{expected_key}`")));
    }
    decode_v2_value(value)
}

fn parse_v2_entry(line: &str) -> Result<ProjectLockDependencyEntry, ProjectError> {
    let body = line
        .strip_prefix("- ")
        .ok_or_else(|| ProjectError::Validation("invalid v2 lockfile dependency line".into()))?;
    let fields: Vec<&str> = body.split(';').collect();
    if fields.len() < 2 {
        return Err(ProjectError::Validation("v2 lockfile dependency lacks a source".into()));
    }
    let (source_key, source_value) = fields[1]
        .split_once('=')
        .ok_or_else(|| ProjectError::Validation("v2 lockfile dependency source is malformed".into()))?;
    if source_key != "source" {
        return Err(ProjectError::Validation("v2 lockfile dependency fields are out of order".into()));
    }
    let source = ProjectLockSource::parse(&decode_v2_value(source_value)?)?;
    const COMMON: [&str; 6] = ["name", "source", "project", "manifest", "source_root", "materialized_root"];
    const REGISTRY: [&str; 9] = [
        "name",
        "source",
        "project",
        "manifest",
        "source_root",
        "materialized_root",
        "registry",
        "resolved_version",
        "artifact_digest",
    ];
    let expected: &[&str] = if source == ProjectLockSource::Registry { &REGISTRY } else { &COMMON };
    if fields.len() != expected.len() {
        return Err(ProjectError::Validation("v2 lockfile dependency has missing or extra fields".into()));
    }
    let mut values = Vec::with_capacity(fields.len());
    for (field, expected_key) in fields.into_iter().zip(expected) {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| ProjectError::Validation(format!("v2 lockfile `{expected_key}` is malformed")))?;
        if key != *expected_key {
            return Err(ProjectError::Validation(format!("v2 lockfile dependency expected `{expected_key}`")));
        }
        values.push(decode_v2_value(value)?);
    }
    let path_base = match source {
        ProjectLockSource::Path => PortableLockPathBaseKind::ExternalProject,
        ProjectLockSource::Corelib => PortableLockPathBaseKind::CorelibWorkspace,
        ProjectLockSource::Registry => PortableLockPathBaseKind::LockDirectory,
    };
    let project = PortableLockPath::parse("project", &values[2], path_base)?.as_str().to_string();
    let manifest = PortableLockPath::parse("manifest", &values[3], PortableLockPathBaseKind::ProjectDirectory)?
        .as_str()
        .to_string();
    let source_root = PortableLockPath::parse("source_root", &values[4], PortableLockPathBaseKind::ProjectDirectory)?
        .as_str()
        .to_string();
    let materialized_root =
        PortableLockPath::parse("materialized_root", &values[5], PortableLockPathBaseKind::MaterializedRoot)?
            .as_str()
            .to_string();
    if values[0].is_empty() {
        return Err(ProjectError::Validation("v2 lockfile dependency name is empty".into()));
    }
    let (registry, resolved_version, artifact_digest) = if source == ProjectLockSource::Registry {
        if values[6].is_empty() || values[7].is_empty() || !valid_v2_digest(&values[8]) {
            return Err(ProjectError::Validation("v2 registry lock entry has an invalid pin".into()));
        }
        (Some(values[6].clone()), Some(values[7].clone()), Some(values[8].clone()))
    } else {
        (None, None, None)
    };
    Ok(ProjectLockDependencyEntry {
        name: values[0].clone(),
        source,
        project,
        manifest,
        source_root,
        materialized_root,
        registry,
        resolved_version,
        artifact_digest,
    })
}

fn valid_v2_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else { return false };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_v2_value(value: &str) -> Result<String, ProjectError> {
    if value.is_empty() {
        return Err(ProjectError::Validation("v2 lockfile value is empty".into()));
    }
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(ProjectError::Validation("v2 lockfile has an incomplete percent escape".into()));
            }
            let high = uppercase_hex_digit(bytes[index + 1])?;
            let low = uppercase_hex_digit(bytes[index + 2])?;
            let byte = high * 16 + low;
            if safe_v2_byte(byte) {
                return Err(ProjectError::Validation("v2 lockfile unnecessarily escapes a literal-safe byte".into()));
            }
            decoded.push(byte);
            index += 3;
        } else if safe_v2_byte(bytes[index]) {
            decoded.push(bytes[index]);
            index += 1;
        } else {
            return Err(ProjectError::Validation("v2 lockfile contains a raw noncanonical byte".into()));
        }
    }
    String::from_utf8(decoded).map_err(|_| ProjectError::Validation("v2 lockfile has malformed UTF-8".into()))
}

fn uppercase_hex_digit(byte: u8) -> Result<u8, ProjectError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ProjectError::Validation("v2 lockfile requires uppercase hex escapes".into())),
    }
}

fn safe_v2_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._/:-@+".contains(&byte)
}

fn encode_v2_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if safe_v2_byte(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            encoded.push(char::from(b"0123456789ABCDEF"[(byte & 0x0F) as usize]));
        }
    }
    encoded
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectLockfileV1 {
    root_manifest: String,
    project_name: String,
    dependencies: Vec<ProjectLockDependencyEntry>,
}

impl ProjectLockfileV1 {
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
                if root_manifest.is_some() {
                    return Err(ProjectError::Validation("lockfile duplicates `root_manifest`".to_string()));
                }
                root_manifest = Some(value.to_string());
                continue;
            }
            if let Some(value) = line.strip_prefix("project_name=") {
                if project_name.is_some() {
                    return Err(ProjectError::Validation("lockfile duplicates `project_name`".to_string()));
                }
                project_name = Some(value.to_string());
                continue;
            }
            if line == "dependencies:" {
                if in_dependencies {
                    return Err(ProjectError::Validation("lockfile duplicates `dependencies:`".to_string()));
                }
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

        let dependency_names: HashSet<_> = dependencies.iter().map(ProjectLockDependencyEntry::name).collect();
        if dependency_names.len() != dependencies.len() {
            return Err(ProjectError::Validation("lockfile duplicates a dependency name".to_string()));
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
}

/// Load dependency lines from `project_root/Project.lock` when the file exists.
pub fn load_project_lock_dependencies(project_root: &Path) -> Result<Vec<ProjectLockDependencyEntry>, ProjectError> {
    load_project_lock_dependencies_from_path(&project_root.join(PROJECT_LOCK_FILE_NAME))
}

/// Strictly load the v2 dependency entries from one explicit lockfile path.
///
/// Callers that replay lockfile paths must use this parser rather than scanning
/// individual lines: malformed or duplicate entries invalidate the entire
/// lockfile.
pub fn load_project_lock_dependencies_from_path(
    lock_path: &Path,
) -> Result<Vec<ProjectLockDependencyEntry>, ProjectError> {
    if !regular_lockfile_exists(lock_path)? {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&lock_path)
        .map_err(|e| ProjectError::Validation(format!("failed to read {}: {e}", lock_path.display())))?;
    Ok(ProjectLockfileV2::parse_v2(&content)?.dependencies)
}

/// Replay only a lockfile issued for this exact project. Other callers may inspect
/// dependency entries, but compilation must not trust a copied lock's root identity.
pub fn load_project_lock_dependencies_for_plan(
    lock_path: &Path,
    plan: &CompilePlan,
) -> Result<Vec<ProjectLockDependencyEntry>, ProjectError> {
    regular_lockfile_exists(lock_path)?;
    let content = fs::read_to_string(lock_path)
        .map_err(|e| ProjectError::Validation(format!("failed to read {}: {e}", lock_path.display())))?;
    let parsed = ProjectLockfileV2::parse_v2(&content)?;
    let lock_root = lock_path.parent().ok_or_else(|| ProjectError::Validation("lockfile has no parent".into()))?;
    let root_manifest = lock_root.join(&parsed.root_manifest);
    let same_manifest = match (root_manifest.canonicalize(), plan.manifest_path.canonicalize()) {
        (Ok(actual), Ok(expected)) => actual == expected,
        (Err(_), Err(_)) => root_manifest == plan.manifest_path,
        _ => false,
    };
    if !same_manifest || parsed.project_name != plan.project_name {
        return Err(ProjectError::Validation("lockfile belongs to a different project".into()));
    }
    let verified_corelib_root = super::prepare::verified_installed_corelib_root();
    let path_entries = plan
        .dependency_projects
        .iter()
        .map(|dependency| {
            super::prepare::portable_entry_for_dependency(plan, dependency, verified_corelib_root.as_deref())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let registry_names = plan
        .unresolved_dependencies
        .iter()
        .filter(|dependency| dependency.source == crate::projects::model::DependencySource::Registry)
        .map(|dependency| dependency.dependency_name.as_str())
        .collect::<Vec<_>>();
    validate_existing_lock_graph(Some(&parsed), &path_entries, &registry_names, false)?;
    Ok(parsed.dependencies)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspacePrepareOptions {
    pub frozen: bool,
    pub locked: bool,
    pub refresh_lock: bool,
}

pub(super) fn preflight_existing_lock_for_plan(
    plan: &CompilePlan,
    options: WorkspacePrepareOptions,
) -> Result<Option<ProjectLockfileV2>, ProjectError> {
    let lock_path = plan.project_root.join(PROJECT_LOCK_FILE_NAME);
    if !regular_lockfile_exists(&lock_path)? {
        if options.locked {
            return Err(ProjectError::LockfileRequired { path: lock_path });
        }
        if options.frozen {
            return Err(ProjectError::LockfileFrozenMode);
        }
        return Ok(None);
    }
    let content =
        fs::read_to_string(&lock_path).map_err(|source| ProjectError::LockfileRead { path: lock_path, source })?;
    match content.lines().next() {
        Some(PROJECT_LOCK_HEADER_V1) => {
            if !options.refresh_lock {
                return Err(ProjectError::Validation(
                    "Project.lock v1 requires explicit migration with `beskid lock` or `beskid update`".into(),
                ));
            }
            ProjectLockfileV1::parse_v1(&content)?;
            Ok(None)
        }
        Some(PROJECT_LOCK_HEADER_V2) => {
            let existing = ProjectLockfileV2::parse_v2(&content)?;
            let current = ProjectLockfileV2::from_plan(plan, &[])?;
            if !options.refresh_lock
                && (existing.root_manifest != current.root_manifest || existing.project_name != current.project_name)
            {
                return Err(ProjectError::Validation("lockfile belongs to a different project".into()));
            }
            Ok(Some(existing))
        }
        _ => Err(ProjectError::Validation("unknown Project.lock format".into())),
    }
}

pub(super) fn validate_existing_lock_graph(
    existing: Option<&ProjectLockfileV2>,
    path_entries: &[ProjectLockDependencyEntry],
    registry_names: &[&str],
    refresh_lock: bool,
) -> Result<(), ProjectError> {
    let Some(existing) = existing else { return Ok(()) };
    if refresh_lock {
        return Ok(());
    }
    let existing_paths =
        existing.dependencies.iter().filter(|entry| entry.source != ProjectLockSource::Registry).collect::<Vec<_>>();
    if existing_paths.len() != path_entries.len()
        || path_entries.iter().any(|entry| !existing_paths.iter().any(|locked| *locked == entry))
    {
        return Err(ProjectError::Validation(
            "Project.lock is stale: dependency graph changed; run `beskid update`".into(),
        ));
    }
    let existing_registry_names = existing
        .dependencies
        .iter()
        .filter(|entry| entry.source == ProjectLockSource::Registry)
        .map(|entry| entry.name.as_str())
        .collect::<HashSet<_>>();
    let current_registry_names = registry_names.iter().copied().collect::<HashSet<_>>();
    // A warning-only registry lookup can leave a declared dependency without a
    // pin. Keep that absence replayable while it is still unavailable; the
    // prepare step rejects a newly resolved package before materialization.
    if !existing_registry_names.is_subset(&current_registry_names)
        || registry_names.len() != current_registry_names.len()
    {
        return Err(ProjectError::Validation(
            "Project.lock is stale: registry dependency graph changed; run `beskid update`".into(),
        ));
    }
    Ok(())
}

pub(super) fn sync_project_lockfile(
    plan: &CompilePlan,
    lock_entries: &[ProjectLockDependencyEntry],
    options: WorkspacePrepareOptions,
) -> Result<std::path::PathBuf, ProjectError> {
    let lock_path = plan.project_root.join(PROJECT_LOCK_FILE_NAME);
    let expected_lockfile = ProjectLockfileV2::from_plan(plan, lock_entries)?;
    let expected_content = expected_lockfile.to_v2_content();
    let lock_exists = regular_lockfile_exists(&lock_path)?;

    if options.locked && !lock_exists {
        return Err(ProjectError::LockfileRequired { path: lock_path });
    }

    if lock_exists {
        let existing = fs::read_to_string(&lock_path)
            .map_err(|source| ProjectError::LockfileRead { path: lock_path.clone(), source })?;
        let existing_matches = if existing == expected_content {
            true
        } else {
            ProjectLockfileV2::parse_v2(&existing).map(|parsed| parsed == expected_lockfile).unwrap_or(false)
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
        if !options.refresh_lock && existing.starts_with(PROJECT_LOCK_HEADER_V2) {
            return Err(ProjectError::Validation(
                "Project.lock is stale: dependency graph changed; run `beskid update`".into(),
            ));
        }
    } else if options.frozen {
        return Err(ProjectError::LockfileFrozenMode);
    }

    let mut staged = NamedTempFile::new_in(&plan.project_root)
        .map_err(|source| ProjectError::LockfileWrite { path: lock_path.clone(), source })?;
    staged
        .write_all(expected_content.as_bytes())
        .map_err(|source| ProjectError::LockfileWrite { path: lock_path.clone(), source })?;
    staged
        .persist(&lock_path)
        .map_err(|error| ProjectError::LockfileWrite { path: lock_path.clone(), source: error.error })?;

    Ok(lock_path)
}
