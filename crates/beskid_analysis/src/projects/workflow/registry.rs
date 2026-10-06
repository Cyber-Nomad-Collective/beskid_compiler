use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile};
use zip::ZipArchive;

use super::archive::{extract_zip_to_dir, validate_zip_eocd, verify_materialized_tree};
use super::filesystem::materialized_dependency_id;
use super::lockfile::{PortableLockPath, PortableLockPathBaseKind, ProjectLockDependencyEntry, ProjectLockSource};
use crate::projects::discovery::is_project_manifest_path;
use crate::projects::error::ProjectError;
use crate::projects::graph::WorkspaceResolutionRules;
use crate::projects::model::{MaterializedDependencyProject, UnresolvedDependencyNote};

pub(super) struct ResolvedRegistryDependency {
    pub(super) dependency_name: String,
    pub(super) materialized_relative: String,
    materialized_root: PathBuf,
    artifact: NamedTempFile,
    selected_version: String,
    artifact_digest: String,
    registry_identity: String,
}

struct RegistryArtifactLayout {
    manifest: String,
    source_root: &'static str,
}

impl ResolvedRegistryDependency {
    pub(super) fn lock_entry(&self) -> Result<ProjectLockDependencyEntry, ProjectError> {
        let layout = registry_artifact_layout(&self.artifact)?;
        let staging = tempfile::tempdir().map_err(|error| ProjectError::Validation(error.to_string()))?;
        let staging_root =
            staging.path().canonicalize().map_err(|error| ProjectError::Validation(error.to_string()))?;
        extract_zip_to_dir(
            self.artifact.reopen().map_err(|error| ProjectError::Validation(error.to_string()))?,
            &staging_root,
        )?;
        crate::projects::load_manifest_from_path(&staging_root.join(&layout.manifest))?;
        if self.materialized_root.exists() {
            verify_materialized_tree(&staging_root, &self.materialized_root)?;
        }
        Ok(ProjectLockDependencyEntry {
            name: self.dependency_name.clone(),
            source: ProjectLockSource::Registry,
            project: self.materialized_relative.clone(),
            manifest: layout.manifest,
            source_root: layout.source_root.into(),
            materialized_root: self.materialized_relative.clone(),
            resolved_version: Some(self.selected_version.clone()),
            artifact_digest: Some(self.artifact_digest.clone()),
            registry: Some(self.registry_identity.clone()),
        })
    }

    pub(super) fn cache_artifact(&self, project_root: &Path) -> Result<(), ProjectError> {
        let path = cache_path(project_root, &self.artifact_digest)?;
        for directory in [project_root.join(".beskid"), project_root.join(".beskid/package-cache")] {
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => {
                    return Err(ProjectError::Validation("registry cache directory is not a real directory".into()));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&directory).map_err(|error| ProjectError::Validation(error.to_string()))?
                }
                Err(error) => return Err(ProjectError::Validation(error.to_string())),
            }
        }
        if path.exists() {
            read_cached_artifact(project_root, &self.artifact_digest)?;
            return Ok(());
        }
        let mut staged = Builder::new()
            .prefix("artifact-")
            .tempfile_in(path.parent().unwrap())
            .map_err(|error| ProjectError::Validation(error.to_string()))?;
        std::io::copy(
            &mut self.artifact.reopen().map_err(|error| ProjectError::Validation(error.to_string()))?,
            staged.as_file_mut(),
        )
        .map_err(|error| ProjectError::Validation(error.to_string()))?;
        staged.as_file().sync_all().map_err(|error| ProjectError::Validation(error.to_string()))?;
        if let Err(error) = staged.persist_noclobber(&path) {
            if error.error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(ProjectError::Validation(error.error.to_string()));
            }
            read_cached_artifact(project_root, &self.artifact_digest)?;
        }
        Ok(())
    }
    /// Check every portable path in an existing pin before preparation creates
    /// `obj`. The artifact digest has already been verified by resolution.
    pub(super) fn validate_existing_lock_entry(
        &self,
        pinned: Option<&ProjectLockDependencyEntry>,
    ) -> Result<(), ProjectError> {
        let Some(pinned) = pinned else { return Ok(()) };
        if pinned.project != self.materialized_relative || pinned.materialized_root != self.materialized_relative {
            return Err(stale_registry_pin(&self.dependency_name));
        }

        let layout = registry_artifact_layout(&self.artifact)?;
        if pinned.manifest != layout.manifest || pinned.source_root != layout.source_root {
            return Err(stale_registry_pin(&self.dependency_name));
        }
        Ok(())
    }
}

fn registry_artifact_layout(artifact: &NamedTempFile) -> Result<RegistryArtifactLayout, ProjectError> {
    let mut artifact =
        artifact.reopen().map_err(|_| ProjectError::Validation("failed to reopen registry scratch artifact".into()))?;
    validate_zip_eocd(&mut artifact)?;
    let mut archive = ZipArchive::new(artifact)
        .map_err(|error| ProjectError::Validation(format!("invalid registry artifact ZIP: {error}")))?;
    let mut manifest = None;
    let mut has_lowercase_src = false;
    let mut has_uppercase_src = false;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ProjectError::Validation(format!("failed to read registry artifact entry: {error}")))?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| ProjectError::Validation("registry artifact ZIP has an unsafe entry path".into()))?;
        let mut components = path.components();
        let first = components.next();
        let nested = components.next().is_some();
        if !nested && !entry.is_dir() && is_project_manifest_path(&path) {
            let name = path
                .to_str()
                .ok_or_else(|| ProjectError::Validation("registry artifact manifest name is not UTF-8".into()))?;
            if manifest.replace(name.to_string()).is_some() {
                return Err(ProjectError::Validation("registry artifact has multiple project manifests".into()));
            }
        }
        if nested || entry.is_dir() {
            if first == Some(std::path::Component::Normal(std::ffi::OsStr::new("src"))) {
                has_lowercase_src = true;
            } else if first == Some(std::path::Component::Normal(std::ffi::OsStr::new("Src"))) {
                has_uppercase_src = true;
            }
        }
    }
    let manifest =
        manifest.ok_or_else(|| ProjectError::Validation("registry artifact has no project manifest".into()))?;
    let source_root = if has_lowercase_src {
        "src"
    } else if has_uppercase_src {
        "Src"
    } else {
        "."
    };
    Ok(RegistryArtifactLayout { manifest, source_root })
}

fn stale_registry_pin(dependency_name: &str) -> ProjectError {
    ProjectError::Validation(format!(
        "registry pin for `{dependency_name}` is stale: artifact paths differ from Project.lock; run `beskid update`"
    ))
}

const MAX_REGISTRY_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
const REGISTRY_SCRATCH_PREFIX: &str = ".beskid-registry-artifact-";

enum RegistryArtifactDownloadError {
    Unavailable(ProjectError),
    Hard(ProjectError),
}

impl RegistryArtifactDownloadError {
    fn into_project_error(self) -> ProjectError {
        match self {
            Self::Unavailable(error) | Self::Hard(error) => error,
        }
    }
}

pub(super) fn resolve_registry_dependency(
    unresolved: &UnresolvedDependencyNote,
    workspace_rules: Option<&WorkspaceResolutionRules>,
    pinned: Option<&ProjectLockDependencyEntry>,
    refresh: bool,
    project_root: &Path,
    offline: bool,
    tolerate_unavailable: bool,
) -> Result<Option<ResolvedRegistryDependency>, ProjectError> {
    // An unpinned package that the registry cannot serve stays unresolved (warning-only) during ordinary
    // preparation; pinned, refreshed, and graph-changing resolution fail closed.
    // Offline refresh revalidates eligible pinned bytes; it cannot discover a new
    // coordinate or bypass exact intent checks without registry metadata.
    let refresh = refresh && !offline;
    let tolerate_unavailable = tolerate_unavailable && pinned.is_none() && !refresh;
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
    if let Some(pin) = pinned
        && !refresh
        && let Some(requested) = requested_version.as_deref().filter(|version| *version != "*")
        && pin.resolved_version.as_deref() != Some(requested)
    {
        return Err(ProjectError::Validation(format!(
            "registry pin for `{}` is stale: manifest requests version `{requested}`; run `beskid update`",
            unresolved.dependency_name
        )));
    }
    let base_url = resolve_registry_base_url(workspace_rules, registry_alias.as_deref());
    let selected_version = if !refresh { pinned.and_then(|entry| entry.resolved_version.clone()) } else { None };
    let selected_version = match selected_version {
        Some(version) => version,
        None if offline => {
            return Err(ProjectError::Validation(format!(
                "offline dependency `{}` requires an eligible exact lock pin",
                unresolved.dependency_name
            )));
        }
        None => match select_registry_version(&base_url, &unresolved.dependency_name, requested_version.as_deref()) {
            Ok(version) => version,
            Err(_) if tolerate_unavailable => return Ok(None),
            Err(error) => return Err(error),
        },
    };

    let download_url = registry_url(
        &base_url,
        &["api", "packages", &unresolved.dependency_name, "versions", &selected_version, "download"],
    )?;
    let (artifact, artifact_digest) = if offline {
        let digest = pinned
            .and_then(|pin| pin.artifact_digest.as_deref())
            .ok_or_else(|| ProjectError::Validation("offline dependency has no verified artifact pin".into()))?;
        (read_cached_artifact(project_root, digest)?, digest.to_owned())
    } else if !refresh
        && let Some(pin) = pinned
        && let Some(digest) = pin.artifact_digest.as_deref()
        && cache_path(project_root, digest)?.exists()
    {
        (read_cached_artifact(project_root, digest)?, digest.to_owned())
    } else {
        match http_get_artifact(&download_url, project_root) {
            Ok(downloaded) => downloaded,
            Err(RegistryArtifactDownloadError::Unavailable(_)) if tolerate_unavailable => return Ok(None),
            Err(error) => return Err(error.into_project_error()),
        }
    };
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
    let materialized_name =
        materialized_dependency_id(&unresolved.dependency_name, ProjectLockSource::Registry, &identity)?;
    let materialized_relative = format!("obj/beskid/deps/src/{materialized_name}");
    let materialized_root = PortableLockPath::parse(
        "materialized_root",
        &materialized_relative,
        PortableLockPathBaseKind::MaterializedRoot,
    )?
    .resolve(project_root)?;
    let owned_root = project_root
        .canonicalize()
        .map_err(|_| ProjectError::Validation("project root cannot be resolved".into()))?
        .join("obj/beskid/deps/src");
    if materialized_root.parent() != Some(owned_root.as_path()) {
        return Err(ProjectError::Validation("registry destination escapes its materialization root".into()));
    }
    Ok(Some(ResolvedRegistryDependency {
        dependency_name: unresolved.dependency_name.clone(),
        materialized_relative,
        materialized_root,
        artifact,
        selected_version,
        artifact_digest,
        registry_identity,
    }))
}

pub(super) fn select_stable_intent(
    package: &str,
    registry: Option<&str>,
    rules: Option<&WorkspaceResolutionRules>,
) -> Result<String, ProjectError> {
    if registry.is_some_and(|alias| !rules.is_some_and(|rules| rules.has_registry_alias(alias))) {
        return Err(ProjectError::Validation("unknown workspace registry alias".into()));
    }
    select_registry_version(&resolve_registry_base_url(rules, registry), package, None)
}

fn cache_path(project_root: &Path, digest: &str) -> Result<PathBuf, ProjectError> {
    let hex = digest
        .strip_prefix("sha256:")
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| ProjectError::Validation("invalid registry cache digest".into()))?;
    Ok(project_root.join(".beskid/package-cache").join(format!("sha256-{hex}.zip")))
}

fn read_cached_artifact(project_root: &Path, digest: &str) -> Result<NamedTempFile, ProjectError> {
    let path = cache_path(project_root, digest)?;
    for directory in [project_root.join(".beskid"), project_root.join(".beskid/package-cache")] {
        let metadata = fs::symlink_metadata(directory)
            .map_err(|_| ProjectError::Validation("offline registry cache is absent".into()))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ProjectError::Validation("registry cache directory is unsafe".into()));
        }
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| ProjectError::Validation("offline registry artifact is absent".into()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_REGISTRY_ARTIFACT_BYTES {
        return Err(ProjectError::Validation("offline registry artifact is unsafe".into()));
    }
    let bytes = fs::read(&path).map_err(|error| ProjectError::Validation(error.to_string()))?;
    if format!("sha256:{:x}", Sha256::digest(&bytes)) != digest {
        return Err(ProjectError::Validation("offline registry artifact digest mismatch".into()));
    }
    let mut artifact = Builder::new()
        .prefix(REGISTRY_SCRATCH_PREFIX)
        .tempfile()
        .map_err(|error| ProjectError::Validation(error.to_string()))?;
    artifact.write_all(&bytes).map_err(|error| ProjectError::Validation(error.to_string()))?;
    Ok(artifact)
}

pub(super) fn materialize_registry_dependency(
    resolved: ResolvedRegistryDependency,
) -> Result<(ProjectLockDependencyEntry, MaterializedDependencyProject), ProjectError> {
    let ResolvedRegistryDependency {
        dependency_name,
        materialized_relative,
        materialized_root,
        artifact,
        selected_version,
        artifact_digest,
        registry_identity,
    } = resolved;
    let layout = registry_artifact_layout(&artifact)?;
    let artifact_reader =
        artifact.reopen().map_err(|_| ProjectError::Validation("failed to reopen registry scratch artifact".into()))?;
    let deps_root = materialized_root
        .parent()
        .ok_or_else(|| ProjectError::Validation("registry materialization has no dependency directory".into()))?;
    let staging = Builder::new()
        .prefix(".beskid-registry-stage-")
        .tempdir_in(deps_root)
        .map_err(|source| ProjectError::MaterializationCreateDir { path: deps_root.to_path_buf(), source })?;
    extract_zip_to_dir(artifact_reader, staging.path())?;

    let staged_manifest =
        crate::projects::discovery::discover_project_manifest_in_dir(staging.path())?.ok_or_else(|| {
            ProjectError::Validation(format!(
                "registry artifact for {}:{} missing a `.bproj` manifest",
                dependency_name, selected_version
            ))
        })?;
    let manifest_relative = staged_manifest
        .strip_prefix(staging.path())
        .map_err(|_| ProjectError::Validation("registry artifact manifest escapes its staged project".into()))?
        .to_path_buf();
    if portable_child_path(staging.path(), &staged_manifest)? != layout.manifest {
        return Err(ProjectError::Validation("registry artifact manifest differs from its ZIP layout".into()));
    }
    let source_relative = if layout.source_root == "." { PathBuf::new() } else { PathBuf::from(layout.source_root) };

    match fs::symlink_metadata(&materialized_root) {
        Ok(_) => verify_materialized_tree(staging.path(), &materialized_root)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::rename(staging.path(), &materialized_root)
                .map_err(|source| ProjectError::MaterializationCreateDir { path: materialized_root.clone(), source })?;
        }
        Err(error) => {
            return Err(ProjectError::Validation(format!(
                "registry materialization cannot be inspected at {}: {error}",
                materialized_root.display()
            )));
        }
    }

    let manifest_path = materialized_root.join(manifest_relative);
    let materialized_source_root = materialized_root.join(source_relative);

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

fn select_registry_version(
    base_url: &str,
    package: &str,
    requested_version: Option<&str>,
) -> Result<String, ProjectError> {
    let versions_url = registry_url(base_url, &["api", "packages", package, "versions"])?;
    let versions_json = http_get_text(&versions_url)?;
    let versions: Vec<Value> = serde_json::from_str(&versions_json)
        .map_err(|error| ProjectError::Validation(format!("registry version response is malformed: {error}")))?;
    let requested = requested_version.filter(|version| *version != "*");
    let mut seen = std::collections::BTreeSet::new();
    let mut available = Vec::new();
    for item in versions {
        let malformed =
            || ProjectError::Validation(format!("registry package `{package}` has malformed version metadata"));
        let text = item.get("version").and_then(Value::as_str).ok_or_else(malformed)?;
        let version = semver::Version::parse(text).map_err(|_| malformed())?;
        let yanked = item.get("isYanked").and_then(Value::as_bool).ok_or_else(malformed)?;
        if !seen.insert(text.to_owned()) {
            return Err(ProjectError::Validation(format!(
                "registry package `{package}` has duplicate version metadata"
            )));
        }
        if !yanked && requested.map_or(version.pre.is_empty(), |requested| text == requested) {
            available.push((version, text.to_owned()));
        }
    }
    available
        .into_iter()
        .max_by(|(left, left_text), (right, right_text)| {
            left.cmp_precedence(right).then_with(|| left_text.cmp(right_text))
        })
        .map(|(_, text)| text)
        .ok_or_else(|| {
            ProjectError::Validation(format!("registry package `{package}` has no available requested version"))
        })
}

#[cfg(test)]
mod v06_selection_tests {
    use super::select_registry_version;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    fn select(response: &str, requested: Option<&str>) -> Result<String, super::ProjectError> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let body = response.to_owned();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = [0; 8192];
            assert!(stream.read(&mut request).unwrap() > 0);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let result = select_registry_version(&url, "Numbers", requested);
        server.join().unwrap();
        result
    }

    #[test]
    fn dep06_bare_selection_uses_semantic_precedence_not_response_order() {
        let response = r#"[{"version":"0.9.0","isYanked":false},{"version":"0.10.0","isYanked":false},{"version":"1.0.0-rc.1","isYanked":false},{"version":"2.0.0","isYanked":true}]"#;
        assert_eq!(select(response, None).unwrap(), "0.10.0");
        assert_eq!(select(response, Some("*")).unwrap(), "0.10.0");
        assert_eq!(select(response, Some("1.0.0-rc.1")).unwrap(), "1.0.0-rc.1");
        assert!(select(response, Some("2.0.0")).is_err());
        assert!(select(response, Some("9.0.0")).is_err());
    }

    #[test]
    fn dep06_equal_precedence_has_deterministic_full_version_tie_break() {
        for response in [
            r#"[{"version":"1.0.0+a","isYanked":false},{"version":"1.0.0+z","isYanked":false}]"#,
            r#"[{"version":"1.0.0+z","isYanked":false},{"version":"1.0.0+a","isYanked":false}]"#,
        ] {
            assert_eq!(select(response, None).unwrap(), "1.0.0+z");
        }
    }

    #[test]
    fn dep06_malformed_registry_metadata_never_becomes_available_intent() {
        for response in [
            r#"[{"version":"1.0.0","isYanked":false},{"isYanked":false}]"#,
            r#"[{"version":"not-a-version","isYanked":false}]"#,
            r#"[{"version":"1.0.0","isYanked":"false"}]"#,
            r#"[{"version":"1.0.0"}]"#,
            r#"[{"version":"1.0.0","isYanked":false},{"version":"1.0.0","isYanked":true}]"#,
        ] {
            assert!(select(response, None).is_err(), "must reject malformed metadata: {response}");
        }
        assert!(select(r#"[{"version":"1.0.0-rc.1","isYanked":false}]"#, None).is_err());
    }
}

fn registry_url(base_url: &str, segments: &[&str]) -> Result<String, ProjectError> {
    let mut url =
        reqwest::Url::parse(base_url).map_err(|_| ProjectError::Validation("registry base URL is invalid".into()))?;
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
    let relative = child
        .strip_prefix(root)
        .map_err(|_| ProjectError::Validation("registry artifact path escapes its materialized project".into()))?;
    let parts = relative
        .components()
        .map(|part| {
            part.as_os_str()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| ProjectError::Validation("registry artifact path is not UTF-8".into()))
        })
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
    let response = client.get(url).send().map_err(|_| ProjectError::Validation("registry request failed".into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProjectError::Validation(format!("registry request failed with status {status}")));
    }
    response.text().map_err(|_| ProjectError::Validation("failed to read registry response".into()))
}

fn artifact_limit_error() -> RegistryArtifactDownloadError {
    RegistryArtifactDownloadError::Hard(ProjectError::Validation(
        "registry artifact exceeds the 64 MiB compressed size limit".into(),
    ))
}

fn http_get_artifact(
    url: &str,
    _scratch_root: &Path,
) -> Result<(NamedTempFile, String), RegistryArtifactDownloadError> {
    let client =
        reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(20)).build().map_err(|_| {
            RegistryArtifactDownloadError::Unavailable(ProjectError::Validation(
                "failed to build registry client".into(),
            ))
        })?;
    let mut response = client.get(url).send().map_err(|_| {
        RegistryArtifactDownloadError::Unavailable(ProjectError::Validation("registry request failed".into()))
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(RegistryArtifactDownloadError::Unavailable(ProjectError::Validation(format!(
            "registry request failed with status {status}"
        ))));
    }
    if response.content_length().is_some_and(|length| length > MAX_REGISTRY_ARTIFACT_BYTES) {
        return Err(artifact_limit_error());
    }
    let mut artifact = Builder::new().prefix(REGISTRY_SCRATCH_PREFIX).tempfile().map_err(|_| {
        RegistryArtifactDownloadError::Hard(ProjectError::Validation(
            "failed to create registry scratch artifact".into(),
        ))
    })?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = response.read(&mut buffer).map_err(|_| {
            RegistryArtifactDownloadError::Unavailable(ProjectError::Validation(
                "failed to read registry response bytes".into(),
            ))
        })?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_REGISTRY_ARTIFACT_BYTES {
            return Err(artifact_limit_error());
        }
        artifact.as_file_mut().write_all(&buffer[..count]).map_err(|_| {
            RegistryArtifactDownloadError::Hard(ProjectError::Validation(
                "failed to write registry scratch artifact".into(),
            ))
        })?;
        digest.update(&buffer[..count]);
    }
    artifact.as_file_mut().flush().map_err(|_| {
        RegistryArtifactDownloadError::Hard(ProjectError::Validation(
            "failed to flush registry scratch artifact".into(),
        ))
    })?;
    Ok((artifact, format!("sha256:{:x}", digest.finalize())))
}
