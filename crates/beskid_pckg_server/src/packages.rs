//! HTTP package-registry routes backed by the pckg persistence boundary.

use axum::{
    Json,
    body::{Body, Bytes, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use beskid_pckg_artifacts::{
    ArtifactRecord, PackageArtifactStore, PublishRequest, select_download, validate_package_artifact,
};
use beskid_pckg_contract::{
    ApiErrorResponse, PackageDetailsResponse, PackageHealthSnapshotResponse, PackageSearchResponse,
    PackageSummaryResponse, PackageVersionLifecycleResponse, PackageVersionSummaryResponse,
    PublishPackageVersionRequest, UpsertPackageRequest,
};
use beskid_pckg_store::{
    NewPackage, NewRegistryActivity, Package, PackageCommunityReview, PackageVersion, PublishOutcome, PublishVersion,
    StoreError,
};

use crate::{AppState, authenticated_subject};

mod artifacts;
mod catalog;
mod contracts;
mod mapping;
mod publishing;
mod reviews;
mod versions;

pub use self::artifacts::{download_artifact, upload_artifact};
pub use self::catalog::{
    delete_package, list_packages, list_publishers, package_detail, publisher_packages, search_packages, upsert_package,
};
pub(crate) use self::contracts::{CommunityReviewRequest, ListQuery, PackageVersionPath};
pub use self::publishing::publish_version;
pub use self::reviews::{create_community_review, list_community_reviews};
pub use self::versions::{list_versions, unyank_version, yank_version};
