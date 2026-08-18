//! Package registry persistence boundary.
//!
//! Domain contracts remain available from the crate root while focused modules
//! own their canonical persistence implementations. Owners are stable Authelia
//! subjects (the `Remote-User` claim, or a carried-over `github:<numeric-id>`
//! from the prior Auth Hub model), never legacy ASP.NET Identity ids.

mod administration;
mod api_keys;
mod memory;
pub mod migrations;
mod operations;
mod package;
mod package_reviews;
mod sql;

pub use administration::{
    AdminRole, AdministrationStoreError, AsyncAdministrationRepository, AsyncPackageReviewRepository,
    PackageReviewDecision, PackageReviewQueueError, PackageReviewRequest, PublisherVerification,
    ResourcePermissionGrant,
};
pub use api_keys::{ApiKey, ApiKeyStoreError, AsyncApiKeyRepository, NewApiKey};
pub use memory::InMemoryPackageRepository;
pub use operations::{
    AsyncRegistryOperationsRepository, BlockedLinkPolicy, NewBlockedLinkPolicy, NewRegistryActivity, RegistryActivity,
    RegistryOperationsStoreError, WeeklySpotlightRun,
};
pub use package::{
    AsyncPackageRepository, NewPackage, Package, PackageRepository, PackageVersion, PublishOutcome, PublishVersion,
    SqlxPackageRepository, StoreError, WorkspacePublishOutcome, WorkspacePublishReservation,
};
pub use package_reviews::{AsyncPackageCommunityReviewRepository, PackageCommunityReview, PackageCommunityReviewError};

#[cfg(test)]
mod tests;
