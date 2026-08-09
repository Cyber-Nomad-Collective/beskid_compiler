use std::fs;
use std::path::{Path, PathBuf};

use crate::abi_v5::TargetMetadata;

use super::hashing::sha256_file;
use super::model::{BuildProfile, ResolvedRuntimeKit, RuntimeArtifact, RuntimeKitMetadata, RuntimeKitResolutionError};
use super::paths::{INSTALLED_RUNTIME_ROOT, profile_directory};

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
