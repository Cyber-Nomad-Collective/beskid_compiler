//! Package registry persistence boundary.
//!
//! Domain contracts remain available from the crate root while focused modules
//! own their canonical persistence implementations. All owners are stable Auth
//! Hub subjects (for example, `github:12345`), never legacy ASP.NET Identity
//! ids.

mod administration;
mod api_keys;
mod community;
mod cutover;
mod memory;
pub mod migrations;
mod operations;
mod package;
mod sql;

pub use administration::{
    AdminRole, AdminRoleAssignment, AdministrationStoreError, AsyncAdministrationRepository,
    AsyncPackageReviewRepository, PackageReviewDecision, PackageReviewQueueError, PackageReviewRequest,
    PublisherVerification, ResourcePermissionGrant,
};
pub use api_keys::{ApiKey, ApiKeyStoreError, AsyncApiKeyRepository, NewApiKey};
pub use community::{
    AsyncCommunityRepository, AsyncPackageCommunityReviewRepository, CommunityBoard, CommunityComment,
    CommunityNotification, CommunityNotificationPreference, CommunityPost, CommunityProfile, CommunityStoreError,
    CommunityVote, NewCommunityNotification, PackageCommunityReview, PackageCommunityReviewError,
    SqlxCommunityRepository,
};
pub use cutover::{
    LegacyIdentityCutoverError, LegacyIdentityCutoverReport, LegacyIdentityCutoverRequest, LegacyIdentityCutoverStatus,
    LegacyIdentitySubjectMapping, UnmappedLegacyIdentity,
};
pub use memory::InMemoryPackageRepository;
pub use operations::{
    AsyncRegistryOperationsRepository, BlockedLinkPolicy, NewBlockedLinkPolicy, NewRegistryActivity, RegistryActivity,
    RegistryOperationsStoreError, WeeklySpotlightRun,
};
pub use package::{
    AsyncPackageRepository, NewPackage, Package, PackageRepository, PackageVersion, PublishOutcome, PublishVersion,
    SqlxPackageRepository, StoreError, WorkspacePublishOutcome, WorkspacePublishReservation,
};

#[cfg(test)]
pub(crate) use community::CREATE_TEST_NOTIFICATION_PROFILE_SQL;
#[cfg(test)]
pub(crate) use cutover::validate_cutover_request;
#[cfg(test)]
mod tests;
