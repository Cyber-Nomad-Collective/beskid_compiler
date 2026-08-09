use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::abi_v5::{ABI_V5, AbiManifestV5, RuntimeAuditMetadata};

use super::hashing::sha256_file;
use super::model::{
    ResolvedRuntimeKit, RuntimeArtifact, RuntimeArtifacts, RuntimeKitBuildError, RuntimeKitBuildRequest,
    RuntimeKitMetadata, RuntimeKitValidationError,
};
use super::paths::{INSTALLED_RUNTIME_ROOT, RUNTIME_KIT_SCHEMA_VERSION, artifact_paths_for_target, profile_directory};
use super::resolution::resolve_installed_runtime_kit;
use super::validation::validate_sha256;

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
