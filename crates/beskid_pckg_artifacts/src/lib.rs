//! Safe package artifact validation and local artifact storage.
//!
//! This crate owns byte-level invariants only.  Package authorization,
//! publication idempotency, and version metadata remain server/store concerns.

mod browse;
mod errors;
mod manifest;
mod model;
mod project_manifest;
mod store;
mod validate;
mod zip_support;

pub use browse::ArtifactBrowser;
pub use errors::ArtifactError;
pub use manifest::parse_package_manifest_metadata;
pub use model::{
    ArtifactDependency, ArtifactDocumentation, ArtifactRecord, BrowseEntry, PackageKind, PackageManifestMetadata,
    PublishRequest, StoredArtifact, ValidatedArtifact, select_download,
};
pub use project_manifest::canonicalize_project_dependencies;
pub use store::{LocalFileArtifactStore, PackageArtifactStore};
pub use validate::validate_package_artifact;
pub use zip_support::MAX_BROWSE_READ_BYTES;
