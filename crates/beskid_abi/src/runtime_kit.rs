//! Serializable metadata for installed native ABI-v5 runtime kits.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::abi_v5::{ABI_V5, AbiManifestV5, RuntimeAuditMetadata, TargetMetadata, TargetValidationError};

pub const RUNTIME_KIT_SCHEMA_VERSION: u32 = 1;

/// Process environment override for the exact installed toolchain prefix.
///
/// When unset, consumers derive the prefix from the current executable
/// (`<prefix>/bin/<tool>` → `<prefix>`). There is no search-path or nearest-kit fallback.
pub const ENV_RUNTIME_PREFIX: &str = "BESKID_RUNTIME_PREFIX";

const INSTALLED_RUNTIME_ROOT: &str = "lib/beskid-runtime/abi-5";

/// Relative installed root shared by every ABI-v5 consumer (`lib/beskid-runtime/abi-5`).
pub fn installed_runtime_root() -> &'static str {
    INSTALLED_RUNTIME_ROOT
}

/// Exact profile directory name under the target kit root.
pub fn profile_directory_name(profile: BuildProfile) -> &'static str {
    profile_directory(profile)
}

/// Path to `abi.json` for one exact prefix/target/profile coordinate.
pub fn exact_kit_metadata_path(prefix: &Path, target: &TargetMetadata, profile: BuildProfile) -> PathBuf {
    prefix.join(INSTALLED_RUNTIME_ROOT).join(target.triple.as_str()).join(profile_directory(profile)).join("abi.json")
}

#[derive(Debug)]
pub enum InstalledRuntimePrefixError {
    CurrentExe(std::io::Error),
    MissingParent { executable: PathBuf },
    MissingInstallPrefix { executable: PathBuf },
}

impl std::fmt::Display for InstalledRuntimePrefixError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentExe(error) => {
                write!(formatter, "cannot locate current executable for ABI-v5 runtime prefix: {error}")
            }
            Self::MissingParent { executable } => {
                write!(formatter, "current executable has no parent: `{}`", executable.display())
            }
            Self::MissingInstallPrefix { executable } => {
                write!(formatter, "current executable has no install prefix: `{}`", executable.display())
            }
        }
    }
}

impl std::error::Error for InstalledRuntimePrefixError {}

/// Resolve the exact installed prefix: `BESKID_RUNTIME_PREFIX`, else parent of the executable's directory.
pub fn installed_runtime_prefix() -> Result<PathBuf, InstalledRuntimePrefixError> {
    if let Some(prefix) = std::env::var_os(ENV_RUNTIME_PREFIX) {
        return Ok(PathBuf::from(prefix));
    }
    let executable = std::env::current_exe().map_err(InstalledRuntimePrefixError::CurrentExe)?;
    installed_runtime_prefix_for_executable(&executable)
}

/// Derive the install prefix for a known executable path (`<prefix>/bin/<tool>`).
pub fn installed_runtime_prefix_for_executable(executable: &Path) -> Result<PathBuf, InstalledRuntimePrefixError> {
    let bin = executable
        .parent()
        .ok_or_else(|| InstalledRuntimePrefixError::MissingParent { executable: executable.to_path_buf() })?;
    bin.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| InstalledRuntimePrefixError::MissingInstallPrefix { executable: executable.to_path_buf() })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRuntimeTargetError {
    UnsupportedHost { arch: String, os: String },
    UnsupportedTarget { triple: String },
}

impl std::fmt::Display for HostRuntimeTargetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedHost { arch, os } => {
                write!(formatter, "unsupported ABI-v5 runtime host `{arch}-{os}`")
            }
            Self::UnsupportedTarget { triple } => {
                write!(formatter, "unsupported ABI-v5 runtime target `{triple}`")
            }
        }
    }
}

impl std::error::Error for HostRuntimeTargetError {}

/// Triple string for the native ABI-v5 host, when the OS/arch pair is supported.
pub fn host_runtime_triple() -> Result<&'static str, HostRuntimeTargetError> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Ok("x86_64-unknown-linux-gnu"),
        ("aarch64", "macos") => Ok("aarch64-apple-darwin"),
        ("x86_64", "windows") => Ok("x86_64-pc-windows-msvc"),
        (arch, os) => Err(HostRuntimeTargetError::UnsupportedHost { arch: arch.into(), os: os.into() }),
    }
}

/// Canonical [`TargetMetadata`] for the native ABI-v5 host.
pub fn host_runtime_target() -> Result<TargetMetadata, HostRuntimeTargetError> {
    let triple = host_runtime_triple()?;
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == triple)
        .ok_or_else(|| HostRuntimeTargetError::UnsupportedTarget { triple: triple.into() })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifact {
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifacts {
    pub static_library: RuntimeArtifact,
    pub shared_library: RuntimeArtifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_import_library: Option<RuntimeArtifact>,
}

impl RuntimeArtifacts {
    fn iter(&self) -> impl Iterator<Item = &RuntimeArtifact> {
        [&self.static_library, &self.shared_library].into_iter().chain(self.shared_import_library.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeKitMetadata {
    pub schema_version: u32,
    pub abi_version: u32,
    pub target: TargetMetadata,
    pub profile: BuildProfile,
    pub layout_hash: String,
    pub source_hash: String,
    pub artifacts: RuntimeArtifacts,
    pub import_allowlist: Vec<String>,
    pub export_allowlist: Vec<String>,
    pub loader_required_exports: Vec<String>,
    pub abi_contract: AbiManifestV5,
    pub audit: RuntimeAuditMetadata,
}

impl RuntimeKitMetadata {
    pub fn canonical_abi_json(&self) -> Result<String, RuntimeKitValidationError> {
        self.validate()?;
        let mut output =
            serde_json::to_string_pretty(self).map_err(|_| RuntimeKitValidationError::InvalidAbiContract)?;
        output.push('\n');
        Ok(output)
    }

    pub fn validate(&self) -> Result<(), RuntimeKitValidationError> {
        if self.schema_version != RUNTIME_KIT_SCHEMA_VERSION {
            return Err(RuntimeKitValidationError::WrongSchemaVersion(self.schema_version));
        }
        if self.abi_version != ABI_V5 {
            return Err(RuntimeKitValidationError::WrongAbiVersion(self.abi_version));
        }
        self.target.validate().map_err(RuntimeKitValidationError::InvalidTarget)?;
        self.abi_contract.validate().map_err(|_| RuntimeKitValidationError::InvalidAbiContract)?;
        if self.abi_contract.target != self.target || self.abi_contract.abi_version != self.abi_version {
            return Err(RuntimeKitValidationError::ContractTargetMismatch);
        }
        if self.abi_contract != AbiManifestV5::canonical_runtime(self.target.clone()) {
            return Err(RuntimeKitValidationError::InvalidAbiContract);
        }
        for (name, hash) in [("layout_hash", &self.layout_hash), ("source_hash", &self.source_hash)] {
            validate_sha256(name, hash)?;
        }

        for artifact in self.artifacts.iter() {
            if !is_portable_relative_path(&artifact.relative_path) {
                return Err(RuntimeKitValidationError::InvalidArtifactPath(artifact.relative_path.clone()));
            }
            validate_sha256("artifact.sha256", &artifact.sha256)?;
        }

        let (static_path, shared_path, import_path) = artifact_paths_for_target(&self.target);
        let actual_import_path =
            self.artifacts.shared_import_library.as_ref().map(|artifact| artifact.relative_path.as_str());
        if self.artifacts.static_library.relative_path != static_path
            || self.artifacts.shared_library.relative_path != shared_path
            || actual_import_path != import_path
        {
            return Err(RuntimeKitValidationError::InvalidArtifactSet { target: self.target.triple.as_str().into() });
        }

        validate_allowlist(&self.import_allowlist)?;
        validate_allowlist(&self.export_allowlist)?;
        validate_allowlist(&self.loader_required_exports)?;
        if self.layout_hash != self.abi_contract.layout_hash() || self.layout_hash != self.audit.layout_hash {
            return Err(RuntimeKitValidationError::ContractLayoutHashMismatch { actual: self.layout_hash.clone() });
        }
        if self.source_hash != self.audit.runtime_source_hash {
            return Err(RuntimeKitValidationError::ContractSourceHashMismatch { actual: self.source_hash.clone() });
        }
        self.audit
            .validate(&self.abi_contract)
            .map_err(|_| RuntimeKitValidationError::ContractAuditMismatch { field: "audit".into() })?;
        if self.import_allowlist != self.audit.allowed_imports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch { field: "import_allowlist".into() });
        }
        if self.export_allowlist != self.audit.allowed_exports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch { field: "export_allowlist".into() });
        }
        if self.loader_required_exports != self.audit.loader_required_exports {
            return Err(RuntimeKitValidationError::ContractAuditMismatch { field: "loader_required_exports".into() });
        }
        Ok(())
    }
}

fn is_portable_relative_path(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value.as_bytes().get(1) == Some(&b':')
    {
        return false;
    }
    value.split('/').all(|component| !component.is_empty() && component != "." && component != "..")
}

fn validate_sha256(name: &str, value: &str) -> Result<(), RuntimeKitValidationError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(RuntimeKitValidationError::InvalidSha256 { field: name.into() });
    }
    Ok(())
}

fn validate_allowlist(symbols: &[String]) -> Result<(), RuntimeKitValidationError> {
    let mut seen = HashSet::new();
    for symbol in symbols {
        if symbol.is_empty() || !seen.insert(symbol.as_str()) {
            return Err(RuntimeKitValidationError::DuplicateAllowlistSymbol { symbol: symbol.clone() });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeKitValidationError {
    WrongSchemaVersion(u32),
    WrongAbiVersion(u32),
    InvalidTarget(TargetValidationError),
    InvalidSha256 { field: String },
    InvalidArtifactSet { target: String },
    InvalidArtifactPath(String),
    DuplicateAllowlistSymbol { symbol: String },
    InvalidAbiContract,
    ContractTargetMismatch,
    ContractLayoutHashMismatch { actual: String },
    ContractSourceHashMismatch { actual: String },
    ContractAuditMismatch { field: String },
}

#[derive(Debug)]
pub struct ResolvedRuntimeKit {
    pub root: PathBuf,
    pub metadata: RuntimeKitMetadata,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

#[derive(Debug)]
pub enum RuntimeKitResolutionError {
    RequestedTarget(TargetValidationError),
    MetadataRead { path: PathBuf, source: std::io::Error },
    MetadataDecode { path: PathBuf, source: serde_json::Error },
    MetadataValidation(RuntimeKitValidationError),
    TargetMismatch { requested: String, actual: String },
    ProfileMismatch { requested: BuildProfile, actual: BuildProfile },
    ArtifactRead { path: PathBuf, source: std::io::Error },
    ArtifactNotRegularFile { path: PathBuf },
    ArtifactHashMismatch { path: PathBuf, expected: String, actual: String },
}

#[derive(Debug, Clone)]
pub struct RuntimeKitBuildRequest {
    pub prefix: PathBuf,
    pub target: TargetMetadata,
    pub profile: BuildProfile,
    pub runtime_source_hash: String,
    pub static_library: PathBuf,
    pub shared_library: PathBuf,
    pub shared_import_library: Option<PathBuf>,
}

#[derive(Debug)]
pub enum RuntimeKitBuildError {
    InvalidTarget(TargetValidationError),
    InvalidSourceHash,
    InvalidArtifactSet { target: String },
    SourceArtifactRead { path: PathBuf, source: std::io::Error },
    SourceArtifactNotRegularFile { path: PathBuf },
    DestinationExists { path: PathBuf },
    DestinationWrite { path: PathBuf, source: std::io::Error },
    CopiedArtifactHashMismatch { path: PathBuf, expected: String, actual: String },
    Metadata(RuntimeKitValidationError),
    Resolution(RuntimeKitResolutionError),
}

pub fn build_runtime_kit(request: &RuntimeKitBuildRequest) -> Result<ResolvedRuntimeKit, RuntimeKitBuildError> {
    request.target.validate().map_err(RuntimeKitBuildError::InvalidTarget)?;
    validate_sha256("runtime_source_hash", &request.runtime_source_hash)
        .map_err(|_| RuntimeKitBuildError::InvalidSourceHash)?;

    let (static_relative, shared_relative, import_relative) = artifact_paths_for_target(&request.target);
    if request.shared_import_library.is_some() != import_relative.is_some() {
        return Err(RuntimeKitBuildError::InvalidArtifactSet { target: request.target.triple.as_str().into() });
    }

    let static_hash = source_artifact_hash(&request.static_library)?;
    let shared_hash = source_artifact_hash(&request.shared_library)?;
    let import_hash = request.shared_import_library.as_ref().map(|path| source_artifact_hash(path)).transpose()?;
    let artifacts = RuntimeArtifacts {
        static_library: RuntimeArtifact { relative_path: static_relative.into(), sha256: static_hash },
        shared_library: RuntimeArtifact { relative_path: shared_relative.into(), sha256: shared_hash },
        shared_import_library: import_relative
            .zip(import_hash)
            .map(|(relative_path, sha256)| RuntimeArtifact { relative_path: relative_path.into(), sha256 }),
    };
    let abi_contract = AbiManifestV5::canonical_runtime(request.target.clone());
    let audit = RuntimeAuditMetadata::for_manifest(&abi_contract, &request.runtime_source_hash)
        .map_err(|_| RuntimeKitBuildError::Metadata(RuntimeKitValidationError::InvalidAbiContract))?;
    let metadata = RuntimeKitMetadata {
        schema_version: RUNTIME_KIT_SCHEMA_VERSION,
        abi_version: ABI_V5,
        target: request.target.clone(),
        profile: request.profile,
        layout_hash: abi_contract.layout_hash(),
        source_hash: request.runtime_source_hash.clone(),
        artifacts,
        import_allowlist: audit.allowed_imports.clone(),
        export_allowlist: audit.allowed_exports.clone(),
        loader_required_exports: audit.loader_required_exports.clone(),
        abi_contract,
        audit,
    };
    let abi_json = metadata.canonical_abi_json().map_err(RuntimeKitBuildError::Metadata)?;

    let profile_directory = profile_directory(request.profile);
    let parent = request.prefix.join(INSTALLED_RUNTIME_ROOT).join(request.target.triple.as_str());
    let destination = parent.join(profile_directory);
    if destination.exists() {
        return Err(RuntimeKitBuildError::DestinationExists { path: destination });
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let staging = parent.join(format!(".{profile_directory}.staging-{}-{nonce}", std::process::id()));
    let publish_result = (|| {
        copy_artifact(
            &request.static_library,
            &staging.join(static_relative),
            &metadata.artifacts.static_library.sha256,
        )?;
        copy_artifact(
            &request.shared_library,
            &staging.join(shared_relative),
            &metadata.artifacts.shared_library.sha256,
        )?;
        if let (Some(source), Some(relative)) = (&request.shared_import_library, import_relative) {
            let expected =
                &metadata.artifacts.shared_import_library.as_ref().expect("validated import artifact").sha256;
            copy_artifact(source, &staging.join(relative), expected)?;
        }
        let metadata_path = staging.join("abi.json");
        fs::write(&metadata_path, abi_json)
            .map_err(|source| RuntimeKitBuildError::DestinationWrite { path: metadata_path, source })?;
        fs::rename(&staging, &destination)
            .map_err(|source| RuntimeKitBuildError::DestinationWrite { path: destination.clone(), source })?;
        Ok::<(), RuntimeKitBuildError>(())
    })();
    if publish_result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    publish_result?;

    resolve_installed_runtime_kit(&request.prefix, &request.target, request.profile)
        .map_err(RuntimeKitBuildError::Resolution)
}

pub fn resolve_installed_runtime_kit(
    prefix: &Path,
    target: &TargetMetadata,
    profile: BuildProfile,
) -> Result<ResolvedRuntimeKit, RuntimeKitResolutionError> {
    target.validate().map_err(RuntimeKitResolutionError::RequestedTarget)?;
    let profile_directory = profile_directory(profile);
    let root = prefix.join(INSTALLED_RUNTIME_ROOT).join(target.triple.as_str()).join(profile_directory);
    let metadata_path = root.join("abi.json");
    let metadata_json = fs::read_to_string(&metadata_path)
        .map_err(|source| RuntimeKitResolutionError::MetadataRead { path: metadata_path.clone(), source })?;
    let metadata: RuntimeKitMetadata = serde_json::from_str(&metadata_json)
        .map_err(|source| RuntimeKitResolutionError::MetadataDecode { path: metadata_path.clone(), source })?;
    metadata.validate().map_err(RuntimeKitResolutionError::MetadataValidation)?;
    if metadata.target != *target {
        return Err(RuntimeKitResolutionError::TargetMismatch {
            requested: target.triple.as_str().into(),
            actual: metadata.target.triple.as_str().into(),
        });
    }
    if metadata.profile != profile {
        return Err(RuntimeKitResolutionError::ProfileMismatch { requested: profile, actual: metadata.profile });
    }

    let static_library = verify_artifact(&root, &metadata.artifacts.static_library)?;
    let shared_library = verify_artifact(&root, &metadata.artifacts.shared_library)?;
    let shared_import_library = metadata
        .artifacts
        .shared_import_library
        .as_ref()
        .map(|artifact| verify_artifact(&root, artifact))
        .transpose()?;

    Ok(ResolvedRuntimeKit { root, metadata, static_library, shared_library, shared_import_library })
}

fn verify_artifact(root: &Path, artifact: &RuntimeArtifact) -> Result<PathBuf, RuntimeKitResolutionError> {
    let path = root.join(&artifact.relative_path);
    let file_type = fs::symlink_metadata(&path)
        .map_err(|source| RuntimeKitResolutionError::ArtifactRead { path: path.clone(), source })?
        .file_type();
    if !file_type.is_file() {
        return Err(RuntimeKitResolutionError::ArtifactNotRegularFile { path });
    }
    let actual =
        sha256_file(&path).map_err(|source| RuntimeKitResolutionError::ArtifactRead { path: path.clone(), source })?;
    if actual != artifact.sha256 {
        return Err(RuntimeKitResolutionError::ArtifactHashMismatch {
            path,
            expected: artifact.sha256.clone(),
            actual,
        });
    }
    Ok(path)
}

fn profile_directory(profile: BuildProfile) -> &'static str {
    match profile {
        BuildProfile::Debug => "debug",
        BuildProfile::Release => "release",
    }
}

fn artifact_paths_for_target(target: &TargetMetadata) -> (&'static str, &'static str, Option<&'static str>) {
    match target.object_format.as_str() {
        "elf" => ("static/libbeskid_runtime.a", "shared/libbeskid_runtime.so", None),
        "macho" => ("static/libbeskid_runtime.a", "shared/libbeskid_runtime.dylib", None),
        "coff" => ("static/beskid_runtime.lib", "shared/beskid_runtime.dll", Some("shared/beskid_runtime_import.lib")),
        _ => unreachable!("target validation rejects unsupported object formats"),
    }
}

fn source_artifact_hash(path: &Path) -> Result<String, RuntimeKitBuildError> {
    let file_type = fs::symlink_metadata(path)
        .map_err(|source| RuntimeKitBuildError::SourceArtifactRead { path: path.to_path_buf(), source })?
        .file_type();
    if !file_type.is_file() {
        return Err(RuntimeKitBuildError::SourceArtifactNotRegularFile { path: path.to_path_buf() });
    }
    sha256_file(path).map_err(|source| RuntimeKitBuildError::SourceArtifactRead { path: path.to_path_buf(), source })
}

fn copy_artifact(source: &Path, destination: &Path, expected_hash: &str) -> Result<(), RuntimeKitBuildError> {
    let parent = destination.parent().expect("artifact path has a parent");
    fs::create_dir_all(parent)
        .map_err(|source| RuntimeKitBuildError::DestinationWrite { path: parent.to_path_buf(), source })?;
    fs::copy(source, destination)
        .map_err(|source| RuntimeKitBuildError::DestinationWrite { path: destination.to_path_buf(), source })?;
    let actual_hash = sha256_file(destination)
        .map_err(|source| RuntimeKitBuildError::DestinationWrite { path: destination.to_path_buf(), source })?;
    if actual_hash != expected_hash {
        return Err(RuntimeKitBuildError::CopiedArtifactHashMismatch {
            path: destination.to_path_buf(),
            expected: expected_hash.into(),
            actual: actual_hash,
        });
    }
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
