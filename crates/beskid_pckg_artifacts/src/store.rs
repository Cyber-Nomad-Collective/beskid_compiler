use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::errors::{ArtifactError, io_error};
use crate::model::{PublishRequest, StoredArtifact};
use crate::zip_support::{is_sha256, sha256_hex};

pub trait PackageArtifactStore {
    fn save(&self, request: PublishRequest<'_>) -> Result<StoredArtifact, ArtifactError>;
    fn open(&self, storage_key: &str) -> Result<Vec<u8>, ArtifactError>;
    fn verify(&self, storage_key: &str, expected_sha256: &str) -> Result<bool, ArtifactError>;
    fn delete(&self, storage_key: &str) -> Result<(), ArtifactError>;
}

/// Filesystem implementation suitable for a single-node deployment.  A future
/// object-store adapter must preserve the key and checksum semantics here.
#[derive(Debug, Clone)]
pub struct LocalFileArtifactStore {
    root: PathBuf,
}

impl LocalFileArtifactStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ArtifactError> {
        let root = fs::canonicalize(root.as_ref())
            .or_else(|_| {
                fs::create_dir_all(root.as_ref())?;
                fs::canonicalize(root.as_ref())
            })
            .map_err(io_error)?;
        Ok(Self { root })
    }

    fn path_for_key(&self, storage_key: &str) -> Result<PathBuf, ArtifactError> {
        let mut parts = storage_key.split('/');
        let (Some(package), Some(version), Some(filename), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ArtifactError::InvalidStorageKey);
        };
        if filename != "artifact.bpk" || !is_storage_component(package) || !is_storage_component(version) {
            return Err(ArtifactError::InvalidStorageKey);
        }
        Ok(self.root.join(package).join(version).join(filename))
    }

    /// Stages immutable artifact bytes without ever replacing an existing
    /// deterministic package/version object. The boolean is true only when
    /// this call created the object, so a caller can safely compensate a later
    /// metadata-transaction failure without deleting another publisher's work.
    pub fn save_staged(&self, request: PublishRequest<'_>) -> Result<(StoredArtifact, bool), ArtifactError> {
        let actual = sha256_hex(request.bytes);
        if actual != request.validated.checksum_sha256 {
            return Err(ArtifactError::ChecksumMismatch);
        }
        let package = storage_component(&request.validated.package_name);
        let version = storage_component(&request.validated.version);
        let storage_key = format!("{package}/{version}/artifact.bpk");
        let path = self.path_for_key(&storage_key)?;
        let stored =
            StoredArtifact { storage_key, checksum_sha256: actual.clone(), size_bytes: request.bytes.len() as u64 };
        match fs::read(&path) {
            Ok(existing) => {
                return if sha256_hex(&existing) == actual {
                    Ok((stored, false))
                } else {
                    Err(ArtifactError::ChecksumMismatch)
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(error)),
        }
        let parent = path.parent().expect("artifact path always has parent");
        fs::create_dir_all(parent).map_err(io_error)?;
        static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let temporary = parent.join(format!(
            ".artifact.bpk.{}.{}.tmp",
            std::process::id(),
            TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::File::create_new(&temporary).and_then(|mut file| file.write_all(request.bytes)).map_err(io_error)?;
        match fs::hard_link(&temporary, &path) {
            Ok(()) => {
                let _ = fs::remove_file(&temporary);
                Ok((stored, true))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temporary);
                let existing = fs::read(&path).map_err(io_error)?;
                if sha256_hex(&existing) == actual { Ok((stored, false)) } else { Err(ArtifactError::ChecksumMismatch) }
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(io_error(error))
            }
        }
    }
}

impl PackageArtifactStore for LocalFileArtifactStore {
    fn save(&self, request: PublishRequest<'_>) -> Result<StoredArtifact, ArtifactError> {
        self.save_staged(request).map(|(stored, _)| stored)
    }

    fn open(&self, storage_key: &str) -> Result<Vec<u8>, ArtifactError> {
        fs::read(self.path_for_key(storage_key)?).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound { ArtifactError::NotFound } else { io_error(error) }
        })
    }

    fn verify(&self, storage_key: &str, expected_sha256: &str) -> Result<bool, ArtifactError> {
        if !is_sha256(expected_sha256) {
            return Ok(false);
        }
        Ok(sha256_hex(&self.open(storage_key)?) == expected_sha256.to_ascii_lowercase())
    }

    fn delete(&self, storage_key: &str) -> Result<(), ArtifactError> {
        let path = self.path_for_key(storage_key)?;
        match fs::remove_file(&path) {
            Ok(()) => {
                if let Some(parent) = path.parent() {
                    let _ = fs::remove_dir(parent);
                }
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }
}

fn storage_component(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') { ch } else { '_' })
        .collect()
}

fn is_storage_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= 200
        && component.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && component != "."
        && component != ".."
}
