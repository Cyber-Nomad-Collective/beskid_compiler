//! AOT artifact cache contract for `type = Mod` projects.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use beskid_codegen::CodegenArtifact;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::{AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode};
use crate::error::{AotError, AotResult};

const MOD_OBJECT_FILE: &str = "mod.o";
const MOD_DESCRIPTOR_FILE: &str = "mod.descriptor.json";
const MOD_DESCRIPTOR_SCHEMA_VERSION: u32 = 1;

/// A contract implementation exported by a built mod artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractRegistration {
    pub contract_id: String,
    pub type_id: String,
    pub entry_symbol: String,
}

/// Descriptor written next to the native object and returned to future mod-host callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModArtifactDescriptor {
    pub schema_version: u32,
    pub package_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_version: Option<String>,
    pub mod_source_hash: String,
    pub lock_hash: String,
    pub target_triple: String,
    pub compiler_version: String,
    pub object_file: String,
    pub registrations: Vec<ContractRegistration>,
    #[serde(skip)]
    pub artifact_key: String,
    #[serde(skip)]
    pub artifact_dir: PathBuf,
}

impl ModArtifactDescriptor {
    /// Absolute native object path inside this descriptor's artifact directory.
    pub fn object_path(&self) -> PathBuf {
        self.artifact_dir.join(&self.object_file)
    }

    /// Absolute sidecar descriptor path inside this descriptor's artifact directory.
    pub fn sidecar_path(&self) -> PathBuf {
        self.artifact_dir.join(MOD_DESCRIPTOR_FILE)
    }
}

/// Inputs for building a workspace-cached mod AOT artifact.
#[derive(Clone)]
pub struct ModArtifactBuildRequest {
    pub artifact: CodegenArtifact,
    pub workspace_root: PathBuf,
    pub project_root: PathBuf,
    pub manifest_path: PathBuf,
    pub source_root: PathBuf,
    pub lockfile_path: Option<PathBuf>,
    pub package_id: String,
    pub package_version: Option<String>,
    pub target_triple: String,
    pub compiler_version: String,
    pub registrations: Vec<ContractRegistration>,
}

impl std::fmt::Debug for ModArtifactBuildRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModArtifactBuildRequest")
            .field("workspace_root", &self.workspace_root)
            .field("project_root", &self.project_root)
            .field("manifest_path", &self.manifest_path)
            .field("source_root", &self.source_root)
            .field("lockfile_path", &self.lockfile_path)
            .field("package_id", &self.package_id)
            .field("package_version", &self.package_version)
            .field("target_triple", &self.target_triple)
            .field("compiler_version", &self.compiler_version)
            .field("registrations", &self.registrations)
            .finish_non_exhaustive()
    }
}

/// Build a mod AOT object and write its sidecar descriptor under the workspace object cache.
pub fn build_mod_artifact(req: ModArtifactBuildRequest) -> AotResult<ModArtifactDescriptor> {
    validate_mod_artifact_request(&req)?;

    let lock_hash = hash_optional_file(req.lockfile_path.as_deref())?;
    let mod_source_hash =
        hash_mod_sources(&req.project_root, &req.manifest_path, &req.source_root)?;
    let artifact_key = compute_mod_artifact_key(
        &lock_hash,
        &mod_source_hash,
        &req.target_triple,
        &req.compiler_version,
    );
    let artifact_dir = mod_artifact_dir(
        &req.workspace_root,
        &req.package_id,
        &artifact_key,
        &req.target_triple,
    );
    fs::create_dir_all(&artifact_dir).map_err(|err| AotError::Io {
        path: artifact_dir.clone(),
        message: err.to_string(),
    })?;

    let object_path = artifact_dir.join(MOD_OBJECT_FILE);
    crate::api::emit_object_only(AotBuildRequest {
        artifact: req.artifact,
        output_kind: BuildOutputKind::ObjectOnly,
        output_path: object_path.clone(),
        object_path: Some(object_path),
        target_triple: Some(req.target_triple.clone()),
        profile: BuildProfile::Debug,
        entrypoint: String::new(),
        export_policy: ExportPolicy::AllDefined,
        link_mode: LinkMode::Auto,
        runtime: crate::api::RuntimeStrategy::Standalone,
        verbose_link: false,
        pipeline: None,
    })?;

    let descriptor = ModArtifactDescriptor {
        schema_version: MOD_DESCRIPTOR_SCHEMA_VERSION,
        package_id: req.package_id,
        package_version: req.package_version,
        mod_source_hash,
        lock_hash,
        target_triple: req.target_triple,
        compiler_version: req.compiler_version,
        object_file: MOD_OBJECT_FILE.to_owned(),
        registrations: req.registrations,
        artifact_key,
        artifact_dir,
    };
    write_descriptor_sidecar(&descriptor)?;
    Ok(descriptor)
}

/// Compute the content-addressed artifact key from the normative cache tuple.
pub fn compute_mod_artifact_key(
    lock_hash: &str,
    mod_source_hash: &str,
    target_triple: &str,
    compiler_version: &str,
) -> String {
    let mut tuple = BTreeMap::new();
    tuple.insert("compiler_version", compiler_version);
    tuple.insert("lock_hash", lock_hash);
    tuple.insert("mod_source_hash", mod_source_hash);
    tuple.insert("target_triple", target_triple);
    let canonical = serde_json::to_vec(&tuple).expect("cache tuple serialization cannot fail");
    sha256_hex(&canonical)
}

/// Workspace object-cache directory for a resolved mod artifact.
pub fn mod_artifact_dir(
    workspace_root: &Path,
    package_id: &str,
    artifact_key: &str,
    target_triple: &str,
) -> PathBuf {
    workspace_root
        .join(".beskid")
        .join("obj")
        .join("mods")
        .join(package_id)
        .join(artifact_key)
        .join(target_triple)
}

fn validate_mod_artifact_request(req: &ModArtifactBuildRequest) -> AotResult<()> {
    for (field, value) in [
        ("package_id", req.package_id.as_str()),
        ("target_triple", req.target_triple.as_str()),
        ("compiler_version", req.compiler_version.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(AotError::InvalidRequest {
                message: format!("mod artifact {field} must not be empty"),
            });
        }
    }

    Ok(())
}

fn write_descriptor_sidecar(descriptor: &ModArtifactDescriptor) -> AotResult<()> {
    let json =
        serde_json::to_string_pretty(descriptor).map_err(|err| AotError::InvalidRequest {
            message: format!("failed to serialize mod artifact descriptor: {err}"),
        })?;
    fs::write(descriptor.sidecar_path(), json).map_err(|err| AotError::Io {
        path: descriptor.sidecar_path(),
        message: err.to_string(),
    })
}

fn hash_optional_file(path: Option<&Path>) -> AotResult<String> {
    match path {
        Some(path) => hash_file(path),
        None => Ok(sha256_hex(&[])),
    }
}

fn hash_mod_sources(
    project_root: &Path,
    manifest_path: &Path,
    source_root: &Path,
) -> AotResult<String> {
    let mut files = vec![manifest_path.to_path_buf()];
    let project_mod = project_root.join("project.mod");
    if project_mod.is_file() {
        files.push(project_mod);
    }
    collect_regular_files(source_root, &mut files)?;
    files.sort();
    files.dedup();

    let mut hasher = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(project_root).unwrap_or(path.as_path());
        let relative = relative.to_string_lossy();
        let contents = fs::read(&path).map_err(|err| AotError::Io {
            path: path.clone(),
            message: err.to_string(),
        })?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(contents.len().to_le_bytes());
        hasher.update([0]);
        hasher.update(&contents);
        hasher.update([0]);
    }

    Ok(hex_digest(hasher.finalize()))
}

fn collect_regular_files(root: &Path, files: &mut Vec<PathBuf>) -> AotResult<()> {
    let entries = fs::read_dir(root).map_err(|err| AotError::Io {
        path: root.to_path_buf(),
        message: err.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|err| AotError::Io {
            path: root.to_path_buf(),
            message: err.to_string(),
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|err| AotError::Io {
            path: path.clone(),
            message: err.to_string(),
        })?;
        if file_type.is_dir() {
            collect_regular_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> AotResult<String> {
    fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|err| AotError::Io {
            path: path.to_path_buf(),
            message: err.to_string(),
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_digest(hasher.finalize())
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
