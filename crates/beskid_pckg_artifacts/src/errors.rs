use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArtifactError {
    #[error("artifact is empty")]
    EmptyArtifact,
    #[error("artifact ZIP is invalid: {0}")]
    InvalidZip(String),
    #[error("artifact manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("artifact checksums are invalid: {0}")]
    InvalidChecksums(String),
    #[error("artifact storage key is invalid")]
    InvalidStorageKey,
    #[error("artifact is missing")]
    NotFound,
    #[error("artifact checksum does not match")]
    ChecksumMismatch,
    #[error("artifact I/O failed: {0}")]
    Io(String),
    #[error("artifact contains an entry that is unsafe to browse: {0}")]
    UnsafeBrowseEntry(String),
    #[error("requested artifact path is not browseable")]
    ForbiddenBrowsePath,
    #[error("artifact entry is missing")]
    EntryNotFound,
    #[error("artifact entry '{path}' exceeds the {limit_bytes} byte read limit")]
    EntryTooLarge { path: String, limit_bytes: u64 },
    #[error("structured documentation metadata is invalid: {0}")]
    InvalidDocumentation(String),
}

pub(crate) fn io_error(error: std::io::Error) -> ArtifactError {
    ArtifactError::Io(error.to_string())
}
