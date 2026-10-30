use std::{
    collections::HashSet,
    fs,
    io::{ErrorKind, Write},
    path::{Component, Path},
};

use tempfile::NamedTempFile;

use crate::projects::{
    error::ProjectError,
    model::{CompilePlan, ProjectWorkspacePlan},
};

mod portable_path;

pub use portable_path::{PortableLockPath, PortableLockPathBaseKind};

pub const PROJECT_LOCK_FILE_NAME: &str = "Project.lock";
const PROJECT_LOCK_HEADER_V3: &str = "# Project.lock v3";

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
    /// The project-local copy beneath `obj/beskid/deps/src`. A Corelib entry has none: Corelib is
    /// read in place from the toolchain's shared Corelib root.
    pub(super) materialized_root: Option<String>,
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

    pub fn materialized_root(&self) -> Option<&str> {
        self.materialized_root.as_deref()
    }

    pub fn manifest(&self) -> &str {
        &self.manifest
    }

    pub fn resolved_version(&self) -> Option<&str> {
        self.resolved_version.as_deref()
    }

    pub fn artifact_digest(&self) -> Option<&str> {
        self.artifact_digest.as_deref()
    }

    pub fn registry(&self) -> Option<&str> {
        self.registry.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLockfile {
    root_manifest: String,
    project_name: String,
    pub(super) dependencies: Vec<ProjectLockDependencyEntry>,
}

impl ProjectLockfile {
    /// A lock that pins nothing beyond the toolchain's Corelib records only what the installed
    /// toolchain re-derives, so it may be regenerated when it no longer matches, for example
    /// after the project folder was copied. Any other entry (a registry pin or a path
    /// dependency) is a choice the user made, and a mismatch there fails.
    pub(super) fn is_pin_free(&self) -> bool {
        self.dependencies.iter().all(|entry| entry.source == ProjectLockSource::Corelib)
    }

    /// Whether this lock was issued for the project the plan describes.
    pub(super) fn belongs_to(&self, current: &ProjectLockfile) -> bool {
        self.root_manifest == current.root_manifest && self.project_name == current.project_name
    }

    /// The error for a lock made for another project; names the pin that cannot be trusted.
    pub(super) fn foreign_lock_error(&self, expected_project: &str) -> ProjectError {
        let base = format!(
            "lockfile belongs to a different project (made for `{}`, this is `{}`)",
            self.project_name, expected_project
        );
        match self.dependencies.iter().find(|entry| entry.source != ProjectLockSource::Corelib) {
            Some(pin) if pin.source == ProjectLockSource::Registry => {
                let version = pin.resolved_version.as_deref().unwrap_or("unversioned");
                ProjectError::Validation(format!(
                    "{base}; its registry pin `{}@{version}` is not trusted here, delete Project.lock to resolve it again",
                    pin.name
                ))
            }
            Some(pin) => ProjectError::Validation(format!(
                "{base}; its path dependency `{}` is not trusted here, delete Project.lock to resolve it again",
                pin.name
            )),
            None => ProjectError::Validation(format!(
                "{base}; it pins nothing beyond Corelib, so `beskid dev project lock` regenerates it"
            )),
        }
    }

    pub(super) fn from_plan(
        plan: &ProjectWorkspacePlan,
        entries: &[ProjectLockDependencyEntry],
    ) -> Result<Self, ProjectError> {
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
            return Err(ProjectError::Validation("registry dependency lacks a complete pin".into()));
        }
        let candidate = Self { root_manifest, project_name: plan.project_name.clone(), dependencies: entries.to_vec() };
        Self::parse(&candidate.to_content())
    }

    pub fn parse(content: &str) -> Result<Self, ProjectError> {
        let body =
            content.strip_suffix('\n').ok_or_else(|| ProjectError::Validation("lockfile must end with LF".into()))?;
        let lines: Vec<&str> = body.split('\n').collect();
        if lines.len() < 4 || lines[0] != PROJECT_LOCK_HEADER_V3 {
            return Err(ProjectError::Validation("lockfile header must be exactly `# Project.lock v3`".into()));
        }
        let root_manifest = parse_top_level(lines[1], "root_manifest")?;
        let root_manifest =
            PortableLockPath::parse("root_manifest", &root_manifest, PortableLockPathBaseKind::LockDirectory)?
                .as_str()
                .to_string();
        let project_name = parse_top_level(lines[2], "project_name")?;
        if lines[3] != "dependencies:" {
            return Err(ProjectError::Validation("lockfile missing `dependencies:`".into()));
        }

        let mut dependencies = Vec::new();
        let mut names = HashSet::new();
        let mut destinations = HashSet::new();
        for line in &lines[4..] {
            let entry = parse_entry(line)?;
            if !names.insert(entry.name.clone()) {
                return Err(ProjectError::Validation("lockfile duplicates a dependency name".into()));
            }
            if let Some(destination) = &entry.materialized_root
                && !destinations.insert(destination.clone())
            {
                return Err(ProjectError::Validation("lockfile duplicates a materialized destination".into()));
            }
            dependencies.push(entry);
        }
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self { root_manifest, project_name, dependencies })
    }

    pub fn to_content(&self) -> String {
        let mut content = format!(
            "{PROJECT_LOCK_HEADER_V3}\nroot_manifest={}\nproject_name={}\ndependencies:\n",
            encode_value(&self.root_manifest),
            encode_value(&self.project_name)
        );
        let mut dependencies = self.dependencies.iter().collect::<Vec<_>>();
        dependencies.sort_by(|left, right| left.name.cmp(&right.name));
        for entry in dependencies {
            content.push_str("- name=");
            content.push_str(&encode_value(&entry.name));
            content.push_str(";source=");
            content.push_str(entry.source.as_str());
            for (key, value) in [
                ("project", Some(&entry.project)),
                ("manifest", Some(&entry.manifest)),
                ("source_root", Some(&entry.source_root)),
                ("materialized_root", entry.materialized_root.as_ref()),
            ] {
                let Some(value) = value else { continue };
                content.push(';');
                content.push_str(key);
                content.push('=');
                content.push_str(&encode_value(value));
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
                    content.push_str(&encode_value(value));
                }
            }
            content.push('\n');
        }
        content
    }
}

fn parse_top_level(line: &str, expected_key: &str) -> Result<String, ProjectError> {
    let (key, value) =
        line.split_once('=').ok_or_else(|| ProjectError::Validation(format!("lockfile missing `{expected_key}`")))?;
    if key != expected_key {
        return Err(ProjectError::Validation(format!("lockfile expected `{expected_key}`")));
    }
    decode_value(value)
}

fn parse_entry(line: &str) -> Result<ProjectLockDependencyEntry, ProjectError> {
    let body =
        line.strip_prefix("- ").ok_or_else(|| ProjectError::Validation("invalid lockfile dependency line".into()))?;
    let fields: Vec<&str> = body.split(';').collect();
    if fields.len() < 2 {
        return Err(ProjectError::Validation("lockfile dependency lacks a source".into()));
    }
    let (source_key, source_value) = fields[1]
        .split_once('=')
        .ok_or_else(|| ProjectError::Validation("lockfile dependency source is malformed".into()))?;
    if source_key != "source" {
        return Err(ProjectError::Validation("lockfile dependency fields are out of order".into()));
    }
    let source = ProjectLockSource::parse(&decode_value(source_value)?)?;
    // A Corelib entry names its package under the shared Corelib root and has no project-local copy.
    const CORELIB: [&str; 5] = ["name", "source", "project", "manifest", "source_root"];
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
    let expected: &[&str] = match source {
        ProjectLockSource::Registry => &REGISTRY,
        ProjectLockSource::Corelib => &CORELIB,
        ProjectLockSource::Path => &COMMON,
    };
    if fields.len() != expected.len() {
        return Err(ProjectError::Validation("lockfile dependency has missing or extra fields".into()));
    }
    let mut values = Vec::with_capacity(fields.len());
    for (field, expected_key) in fields.into_iter().zip(expected) {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| ProjectError::Validation(format!("lockfile `{expected_key}` is malformed")))?;
        if key != *expected_key {
            return Err(ProjectError::Validation(format!("lockfile dependency expected `{expected_key}`")));
        }
        values.push(decode_value(value)?);
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
    let materialized_root = match source {
        ProjectLockSource::Corelib => None,
        ProjectLockSource::Path | ProjectLockSource::Registry => Some(
            PortableLockPath::parse("materialized_root", &values[5], PortableLockPathBaseKind::MaterializedRoot)?
                .as_str()
                .to_string(),
        ),
    };
    if values[0].is_empty() {
        return Err(ProjectError::Validation("lockfile dependency name is empty".into()));
    }
    let (registry, resolved_version, artifact_digest) = if source == ProjectLockSource::Registry {
        if values[6].is_empty() || values[7].is_empty() || !valid_digest(&values[8]) {
            return Err(ProjectError::Validation("registry lock entry has an invalid pin".into()));
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

fn valid_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else { return false };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_value(value: &str) -> Result<String, ProjectError> {
    if value.is_empty() {
        return Err(ProjectError::Validation("lockfile value is empty".into()));
    }
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(ProjectError::Validation("lockfile has an incomplete percent escape".into()));
            }
            let high = uppercase_hex_digit(bytes[index + 1])?;
            let low = uppercase_hex_digit(bytes[index + 2])?;
            let byte = high * 16 + low;
            if safe_lock_byte(byte) {
                return Err(ProjectError::Validation("lockfile unnecessarily escapes a literal-safe byte".into()));
            }
            decoded.push(byte);
            index += 3;
        } else if safe_lock_byte(bytes[index]) {
            decoded.push(bytes[index]);
            index += 1;
        } else {
            return Err(ProjectError::Validation("lockfile contains a raw noncanonical byte".into()));
        }
    }
    String::from_utf8(decoded).map_err(|_| ProjectError::Validation("lockfile has malformed UTF-8".into()))
}

fn uppercase_hex_digit(byte: u8) -> Result<u8, ProjectError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ProjectError::Validation("lockfile requires uppercase hex escapes".into())),
    }
}

fn safe_lock_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"._/:-@+".contains(&byte)
}

fn encode_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if safe_lock_byte(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(char::from(b"0123456789ABCDEF"[(byte >> 4) as usize]));
            encoded.push(char::from(b"0123456789ABCDEF"[(byte & 0x0F) as usize]));
        }
    }
    encoded
}

/// Load dependency lines from `project_root/Project.lock` when the file exists.
pub fn load_project_lock_dependencies(project_root: &Path) -> Result<Vec<ProjectLockDependencyEntry>, ProjectError> {
    load_project_lock_dependencies_from_path(&project_root.join(PROJECT_LOCK_FILE_NAME))
}

/// Strictly load the dependency entries from one explicit lockfile path.
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
    let content = fs::read_to_string(lock_path)
        .map_err(|e| ProjectError::Validation(format!("failed to read {}: {e}", lock_path.display())))?;
    reject_unsupported_lock_format(lock_path, &content)?;
    Ok(ProjectLockfile::parse(&content)?.dependencies)
}

/// A lock whose first line is not the format 3 header (an older format or an unknown one) fails
/// closed: no consumer reads its body, and no command regenerates or rewrites it. The user deletes
/// the file and writes a new lock.
pub(super) fn reject_unsupported_lock_format(lock_path: &Path, content: &str) -> Result<(), ProjectError> {
    if content.split('\n').next() == Some(PROJECT_LOCK_HEADER_V3) {
        return Ok(());
    }
    Err(ProjectError::LockfileFormatUnsupported { path: lock_path.to_path_buf() })
}

/// Replay only a lockfile issued for this exact project. Other callers may inspect
/// dependency entries, but compilation must not trust a copied lock's root identity.
pub fn load_project_lock_dependencies_for_plan(
    lock_path: &Path,
    plan: &CompilePlan,
) -> Result<Vec<ProjectLockDependencyEntry>, ProjectError> {
    let workspace_plan = ProjectWorkspacePlan::from(plan);
    let plan = &workspace_plan;
    regular_lockfile_exists(lock_path)?;
    let content = fs::read_to_string(lock_path)
        .map_err(|e| ProjectError::Validation(format!("failed to read {}: {e}", lock_path.display())))?;
    reject_unsupported_lock_format(lock_path, &content)?;
    let parsed = ProjectLockfile::parse(&content)?;
    let lock_root = lock_path.parent().ok_or_else(|| ProjectError::Validation("lockfile has no parent".into()))?;
    let root_manifest = lock_root.join(&parsed.root_manifest);
    let same_manifest = match (root_manifest.canonicalize(), plan.manifest_path.canonicalize()) {
        (Ok(actual), Ok(expected)) => actual == expected,
        (Err(_), Err(_)) => root_manifest == plan.manifest_path,
        _ => false,
    };
    if !same_manifest || parsed.project_name != plan.project_name {
        // A lock that pins nothing beyond Corelib, copied from another project, is regenerated
        // by the next preparation without a word (`preflight_existing_lock_for_plan`); readers
        // agree and use the declared roots until then instead of reporting it.
        if parsed.is_pin_free() {
            return Ok(Vec::new());
        }
        return Err(parsed.foreign_lock_error(&plan.project_name));
    }
    let shared_corelib = super::shared_corelib::SharedCorelib::at(plan.corelib_root.as_deref());
    let path_entries = plan
        .dependency_projects
        .iter()
        .map(|dependency| super::prepare::portable_entry_for_dependency(plan, dependency, shared_corelib.as_ref()))
        .collect::<Result<Vec<_>, _>>()?;
    let registry_names = plan
        .unresolved_dependencies
        .iter()
        .filter(|dependency| dependency.source == crate::projects::model::DependencySource::Registry)
        .map(|dependency| dependency.dependency_name.as_str())
        .collect::<Vec<_>>();
    validate_existing_lock_graph(Some(&parsed), &path_entries, false)?;
    replay_registry_pins_reachable(lock_root, &parsed, &registry_names)?;
    Ok(parsed.dependencies)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkspacePrepareOptions {
    pub offline: bool,
    pub frozen: bool,
    pub locked: bool,
    pub refresh_lock: bool,
}

pub(super) fn preflight_existing_lock_for_plan(
    plan: &ProjectWorkspacePlan,
    options: WorkspacePrepareOptions,
) -> Result<Option<ProjectLockfile>, ProjectError> {
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
    let content = fs::read_to_string(&lock_path)
        .map_err(|source| ProjectError::LockfileRead { path: lock_path.clone(), source })?;
    // Checked before every other option, `refresh_lock` included: no command rewrites a lock in
    // an unsupported format.
    reject_unsupported_lock_format(&lock_path, &content)?;
    let existing = ProjectLockfile::parse(&content)?;
    let current = ProjectLockfile::from_plan(plan, &[])?;
    if !existing.belongs_to(&current) {
        // A pin-free lock from another project (a copied folder) is regenerated unless the
        // run must not write the lock; pins are never replaced silently.
        if existing.is_pin_free() && !options.locked && !options.frozen {
            return Ok(None);
        }
        return Err(existing.foreign_lock_error(&plan.project_name));
    }
    Ok(Some(existing))
}

/// Whether the path and Corelib entries of `existing` differ from the current project graph.
pub(super) fn lock_graph_differs(existing: &ProjectLockfile, path_entries: &[ProjectLockDependencyEntry]) -> bool {
    let existing_paths =
        existing.dependencies.iter().filter(|entry| entry.source != ProjectLockSource::Registry).collect::<Vec<_>>();
    existing_paths.len() != path_entries.len() || path_entries.iter().any(|entry| !existing_paths.contains(&entry))
}

/// The path and Corelib part of an existing lock must describe the current project graph exactly.
/// Registry pins are checked separately against the resolved registry closure
/// ([`reject_unreachable_registry_pins`]), because transitive registry packages are only known once
/// their requesters' manifests are read.
pub(super) fn validate_existing_lock_graph(
    existing: Option<&ProjectLockfile>,
    path_entries: &[ProjectLockDependencyEntry],
    refresh_lock: bool,
) -> Result<(), ProjectError> {
    let Some(existing) = existing else { return Ok(()) };
    if refresh_lock {
        return Ok(());
    }
    if lock_graph_differs(existing, path_entries) {
        return Err(ProjectError::Validation(
            "Project.lock is stale: dependency graph changed; run `beskid update`".into(),
        ));
    }
    Ok(())
}

/// Every registry pin of an existing lock must belong to the reachable registry closure.
///
/// A declared registry dependency may still lack a pin while the registry cannot serve it
/// (warning-only ordinary preparation), so reachable names need not all be pinned.
pub(super) fn reject_unreachable_registry_pins(
    existing: &ProjectLockfile,
    reachable: &HashSet<&str>,
) -> Result<(), ProjectError> {
    if let Some(entry) = existing
        .dependencies
        .iter()
        .find(|entry| entry.source == ProjectLockSource::Registry && !reachable.contains(entry.name.as_str()))
    {
        return Err(ProjectError::Validation(format!(
            "Project.lock is stale: registry package `{}` is no longer reachable from the declared dependencies; run `beskid update`",
            entry.name
        )));
    }
    Ok(())
}

/// Replay check of registry pins without network or archive access.
///
/// The walk starts at the declared registry dependencies and follows each pinned package's
/// materialized manifest. When a reachable pinned package is not materialized, its requirements
/// cannot be read here, so the walk cannot prove a remaining pin unreachable and accepts the lock;
/// preparation re-resolves the closure from digest-verified artifacts and rejects it there.
fn replay_registry_pins_reachable(
    lock_root: &Path,
    existing: &ProjectLockfile,
    declared_registry_names: &[&str],
) -> Result<(), ProjectError> {
    let pins = existing
        .dependencies
        .iter()
        .filter(|entry| entry.source == ProjectLockSource::Registry)
        .map(|entry| (entry.name.as_str(), entry))
        .collect::<std::collections::HashMap<_, _>>();
    let mut reachable = HashSet::new();
    let mut pending = declared_registry_names.to_vec();
    let mut complete = true;
    while let Some(name) = pending.pop() {
        if !reachable.insert(name) {
            continue;
        }
        let Some(pin) = pins.get(name) else { continue };
        let Some(materialized_root) = pin.materialized_root.as_deref() else {
            complete = false;
            continue;
        };
        let manifest = lock_root.join(materialized_root).join(&pin.manifest);
        let Ok(text) = fs::read_to_string(&manifest) else {
            complete = false;
            continue;
        };
        let parsed = crate::projects::parse_manifest(&text)?;
        for dependency in parsed.dependencies {
            if dependency.source == crate::projects::model::DependencySource::Registry
                && let Some((pinned, _)) = pins.get_key_value(dependency.name.as_str())
            {
                pending.push(*pinned);
            }
        }
    }
    if complete { reject_unreachable_registry_pins(existing, &reachable) } else { Ok(()) }
}

pub(super) fn sync_project_lockfile(
    plan: &ProjectWorkspacePlan,
    lock_entries: &[ProjectLockDependencyEntry],
    options: WorkspacePrepareOptions,
) -> Result<std::path::PathBuf, ProjectError> {
    let lock_path = plan.project_root.join(PROJECT_LOCK_FILE_NAME);
    let expected_lockfile = ProjectLockfile::from_plan(plan, lock_entries)?;
    let expected_content = expected_lockfile.to_content();
    let lock_exists = regular_lockfile_exists(&lock_path)?;

    if options.locked && !lock_exists {
        return Err(ProjectError::LockfileRequired { path: lock_path });
    }

    if lock_exists {
        let existing = fs::read_to_string(&lock_path)
            .map_err(|source| ProjectError::LockfileRead { path: lock_path.clone(), source })?;
        reject_unsupported_lock_format(&lock_path, &existing)?;
        let existing_matches = if existing == expected_content {
            true
        } else {
            ProjectLockfile::parse(&existing).map(|parsed| parsed == expected_lockfile).unwrap_or(false)
        };

        // A lock that says the same in another order (entries edited by hand) is accepted, and
        // an ordinary run writes it back in the canonical order; `--locked` and `--frozen` never
        // rewrite a lock.
        if existing_matches && (existing == expected_content || options.locked || options.frozen) {
            return Ok(lock_path);
        }

        if options.frozen && !existing_matches {
            return Err(ProjectError::LockfileFrozenMode);
        }

        if options.locked && !existing_matches {
            return Err(ProjectError::LockfileOutOfDate { project: plan.project_name.clone() });
        }
        let pin_free = ProjectLockfile::parse(&existing).is_ok_and(|parsed| parsed.is_pin_free());
        if !existing_matches && !options.refresh_lock && !pin_free {
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
