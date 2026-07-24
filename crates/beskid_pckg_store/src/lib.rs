//! Package registry persistence boundary.
//!
//! The production adapter will be backed by PostgreSQL; this crate keeps its
//! domain rules executable without choosing a SQL runtime prematurely.  All
//! owners are stable Auth Hub subjects (for example, `github:12345`), never a
//! legacy ASP.NET Identity id.

use std::collections::BTreeMap;

use async_trait::async_trait;
use beskid_pckg_operations::BlockedLinkPattern;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

// Kept as a named contract because this insert must stay aligned with the
// independently migrated profile table. It creates no synthetic identity data.
const CREATE_TEST_NOTIFICATION_PROFILE_SQL: &str = "INSERT INTO pckg_community_profiles (subject,display_name,bio,social_links,is_publisher_verified,updated_at_utc) VALUES ($1,$1,'','[]'::JSONB,FALSE,$2) ON CONFLICT (subject) DO NOTHING";

pub mod migrations {
    //! Ordered PostgreSQL migration plan for the package-registry cutover.

    /// Creates canonical package and immutable package-version records.
    pub const CREATE_PACKAGE_REGISTRY: &str = include_str!("../migrations/0001_create_package_registry.sql");

    /// Cutover is intentionally explicit: Identity ids need a reviewed mapping
    /// to Auth Hub subjects before this statement can be used in production.
    pub const BACKFILL_REQUIRES_SUBJECT_MAPPING: &str =
        include_str!("../migrations/0002_backfill_requires_subject_mapping.sql");

    /// Stores reviewed legacy-Identity-to-Auth-Hub mappings and every cutover
    /// decision. It intentionally never joins against legacy usernames/emails.
    pub const LEGACY_IDENTITY_CUTOVER_AUDIT: &str =
        include_str!("../migrations/0003_legacy_identity_cutover_audit.sql");

    /// Community profiles, discussions, follows and notifications. All
    /// principals are Auth Hub GitHub subjects, never legacy Identity ids.
    pub const CREATE_COMMUNITY: &str = include_str!("../migrations/0004_create_community.sql");

    /// API keys are pckg-owned automation credentials. The table retains only
    /// a SHA-256 digest, never an issued raw token.
    pub const CREATE_API_KEYS: &str = include_str!("../migrations/0005_create_api_keys.sql");

    /// Roles, publisher verification, resource grants and package-review audit
    /// decisions. This schema intentionally seeds no administrator.
    pub const CREATE_ADMINISTRATION: &str = include_str!("../migrations/0006_create_administration.sql");

    /// Typed community preferences plus the self-addressed system delivery
    /// check. This retains Auth Hub subjects as the only identity key.
    pub const EXTEND_COMMUNITY_NOTIFICATIONS: &str =
        include_str!("../migrations/0007_extend_community_notifications.sql");

    /// Durable review submission queue and the current reviewer disposition.
    pub const CREATE_PACKAGE_REVIEW_QUEUE: &str = include_str!("../migrations/0008_create_package_review_queue.sql");

    /// Registry operations retained after the GitHub-only Auth Hub cutover.
    /// In particular this migration deliberately does not recreate SMTP or
    /// personal-email settings from the retired C# application.
    pub const CREATE_REGISTRY_OPERATIONS: &str = include_str!("../migrations/0009_create_registry_operations.sql");

    pub const CREATE_PACKAGE_COMMUNITY_REVIEWS: &str =
        include_str!("../migrations/0010_create_package_community_reviews.sql");

    pub const ALL: &[(&str, &str)] = &[
        ("0001_create_package_registry", CREATE_PACKAGE_REGISTRY),
        ("0002_backfill_requires_subject_mapping", BACKFILL_REQUIRES_SUBJECT_MAPPING),
        ("0003_legacy_identity_cutover_audit", LEGACY_IDENTITY_CUTOVER_AUDIT),
        ("0004_create_community", CREATE_COMMUNITY),
        ("0005_create_api_keys", CREATE_API_KEYS),
        ("0006_create_administration", CREATE_ADMINISTRATION),
        ("0007_extend_community_notifications", EXTEND_COMMUNITY_NOTIFICATIONS),
        ("0008_create_package_review_queue", CREATE_PACKAGE_REVIEW_QUEUE),
        ("0009_create_registry_operations", CREATE_REGISTRY_OPERATIONS),
        ("0010_create_package_community_reviews", CREATE_PACKAGE_COMMUNITY_REVIEWS),
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedLinkPolicy {
    pub id: String,
    pub pattern: String,
    pub note: Option<String>,
    pub created_by_subject: String,
    pub created_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBlockedLinkPolicy {
    pub id: String,
    pub pattern: String,
    pub note: Option<String>,
    pub created_by_subject: String,
    pub created_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryActivity {
    pub sequence: i64,
    pub occurred_at_unix_seconds: i64,
    pub severity: String,
    pub action: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub actor_subject: Option<String>,
    pub package_name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRegistryActivity {
    pub occurred_at_unix_seconds: i64,
    pub severity: String,
    pub action: String,
    pub message: String,
    pub trace_id: Option<String>,
    pub actor_subject: Option<String>,
    pub package_name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklySpotlightRun {
    pub id: String,
    pub ran_by_subject: String,
    pub ran_at_unix_seconds: i64,
    pub activity_count: u64,
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryOperationsStoreError {
    InvalidAuthHubSubject,
    InvalidBlockedLinkPattern,
    InvalidBlockedLinkId,
    DuplicateBlockedLinkPattern,
    InvalidActivity,
    InvalidWeeklySpotlightRun,
    NotFound,
    Database(String),
}

#[async_trait]
pub trait AsyncRegistryOperationsRepository: Send + Sync {
    async fn list_blocked_link_policies(&self) -> Result<Vec<BlockedLinkPolicy>, RegistryOperationsStoreError>;
    async fn add_blocked_link_policy(
        &self,
        policy: NewBlockedLinkPolicy,
    ) -> Result<BlockedLinkPolicy, RegistryOperationsStoreError>;
    async fn delete_blocked_link_policy(&self, id: &str) -> Result<(), RegistryOperationsStoreError>;
    async fn append_registry_activity(
        &self,
        activity: NewRegistryActivity,
    ) -> Result<RegistryActivity, RegistryOperationsStoreError>;
    async fn recent_registry_activity(&self, take: u16) -> Result<Vec<RegistryActivity>, RegistryOperationsStoreError>;
    async fn record_weekly_spotlight(
        &self,
        run: WeeklySpotlightRun,
    ) -> Result<WeeklySpotlightRun, RegistryOperationsStoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminRole {
    User,
    Moderator,
    SuperAdmin,
}

impl AdminRole {
    fn stored(self) -> Option<&'static str> {
        match self {
            Self::User => None,
            Self::Moderator => Some("moderator"),
            Self::SuperAdmin => Some("superadmin"),
        }
    }
    fn from_stored(value: &str) -> Option<Self> {
        match value {
            "moderator" => Some(Self::Moderator),
            "superadmin" => Some(Self::SuperAdmin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminRoleAssignment {
    pub subject: String,
    pub role: AdminRole,
    pub granted_by_subject: String,
    pub granted_at_unix_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublisherVerification {
    pub subject: String,
    pub is_verified: bool,
    pub reviewed_by_subject: String,
    pub reviewed_at_unix_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePermissionGrant {
    pub subject: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub capability: String,
    pub granted_by_subject: String,
    pub granted_at_unix_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReviewDecision {
    pub package_id: String,
    pub version: Option<String>,
    pub decision: String,
    pub reason: String,
    pub decided_by_subject: String,
    pub decided_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdministrationStoreError {
    InvalidAuthHubSubject,
    InvalidRole,
    InvalidResource,
    InvalidDecision,
    InvalidPackageId,
    Database(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReviewRequest {
    pub id: String,
    pub package_id: String,
    pub requested_by_subject: String,
    pub reason: String,
    pub status: String,
    pub submitted_at_unix_seconds: i64,
    pub reviewer_subject: Option<String>,
    pub review_notes: Option<String>,
    pub reviewed_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageReviewQueueError {
    InvalidAuthHubSubject,
    InvalidPackageId,
    InvalidReviewId,
    InvalidReason,
    InvalidAction,
    NotFound,
    Database(String),
}

#[async_trait]
pub trait AsyncPackageReviewRepository: Send + Sync {
    async fn submit_package_review(
        &self,
        review: PackageReviewRequest,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError>;
    async fn list_package_reviews(&self) -> Result<Vec<PackageReviewRequest>, PackageReviewQueueError>;
    async fn action_package_review(
        &self,
        review_id: &str,
        action: &str,
        reviewer_subject: &str,
        notes: Option<String>,
        reviewed_at_unix_seconds: i64,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError>;
}

#[async_trait]
pub trait AsyncAdministrationRepository: Send + Sync {
    async fn list_admin_roles(&self) -> Result<Vec<AdminRoleAssignment>, AdministrationStoreError>;
    async fn roles_for_subject(&self, subject: &str) -> Result<Vec<AdminRole>, AdministrationStoreError>;
    async fn grant_admin_role(&self, assignment: AdminRoleAssignment) -> Result<(), AdministrationStoreError>;
    async fn replace_admin_roles(
        &self,
        subject: &str,
        roles: Vec<AdminRole>,
        granted_by_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<(), AdministrationStoreError>;
    async fn set_publisher_verification(
        &self,
        verification: PublisherVerification,
    ) -> Result<(), AdministrationStoreError>;
    async fn get_publisher_verification(
        &self,
        subject: &str,
    ) -> Result<Option<PublisherVerification>, AdministrationStoreError>;
    async fn list_publisher_verifications(&self) -> Result<Vec<PublisherVerification>, AdministrationStoreError>;
    async fn grant_resource_permission(&self, grant: ResourcePermissionGrant) -> Result<(), AdministrationStoreError>;
    async fn list_resource_permissions(
        &self,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError>;
    async fn list_all_resource_permissions(&self) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError>;
    async fn record_package_review(&self, decision: PackageReviewDecision) -> Result<(), AdministrationStoreError>;
}

/// API-key metadata that may be shown to its owner. `token_hash` is never
/// returned through this type or any repository method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKey {
    pub id: String,
    pub subject: String,
    pub label: String,
    pub scopes: Vec<String>,
    pub created_at_unix_seconds: i64,
    pub revoked_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewApiKey {
    pub id: String,
    pub subject: String,
    pub label: String,
    pub scopes: Vec<String>,
    /// Present only for the duration of create/verify. Callers must not log or
    /// persist it outside this repository boundary.
    pub raw_token: String,
    pub now_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyStoreError {
    InvalidAuthHubSubject,
    InvalidLabel,
    InvalidScope,
    InvalidToken,
    Database(String),
}

/// Async, subject-scoped persistence for API-key management. All reads and
/// revocations are constrained by caller subject so key ids cannot disclose or
/// alter another owner's credentials.
#[async_trait]
pub trait AsyncApiKeyRepository: Send + Sync {
    async fn create_api_key(&self, request: NewApiKey) -> Result<ApiKey, ApiKeyStoreError>;
    async fn list_api_keys(&self, subject: &str) -> Result<Vec<ApiKey>, ApiKeyStoreError>;
    async fn revoke_api_key(&self, id: &str, subject: &str, now_unix_seconds: i64) -> Result<bool, ApiKeyStoreError>;
    async fn find_active_api_key_by_token(&self, raw_token: &str) -> Result<Option<ApiKey>, ApiKeyStoreError>;
}

/// A reviewed mapping from the old ASP.NET Identity primary key to the only
/// supported pckg principal: a stable GitHub Auth Hub subject.
///
/// Deliberately absent: username, normalized username, email, display name, or
/// any other heuristic source. These mappings must be produced and reviewed by
/// an external identity-export process before calling the cutover runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyIdentitySubjectMapping {
    pub legacy_identity_id: String,
    pub github_subject: String,
    pub approved_by: String,
    pub approved_at_unix_seconds: i64,
}

/// Input to the one-way package ownership cutover. `run_id` is caller-supplied
/// so a deployment/migration runner can correlate its logs with persisted audit
/// rows and retry using a new id only after review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyIdentityCutoverRequest {
    pub run_id: String,
    pub requested_by: String,
    pub mappings: Vec<LegacyIdentitySubjectMapping>,
    pub now_unix_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyIdentityCutoverStatus {
    RejectedUnmappedIdentity,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedLegacyIdentity {
    pub legacy_identity_id: String,
    pub package_count: u64,
}

/// A durable, operator-readable result. Rejections include every legacy owner
/// that prevented import; callers must not attempt a best-effort partial run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyIdentityCutoverReport {
    pub run_id: String,
    pub status: LegacyIdentityCutoverStatus,
    pub mapped_identity_count: u64,
    pub legacy_package_count: u64,
    pub imported_package_count: u64,
    pub imported_version_count: u64,
    pub unmapped_identities: Vec<UnmappedLegacyIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyIdentityCutoverError {
    InvalidRequest(String),
    /// No packages or versions have been imported. The embedded report is also
    /// committed to the audit tables for the migration operator.
    RejectedUnmappedIdentities(LegacyIdentityCutoverReport),
    Store(StoreError),
}

impl From<StoreError> for LegacyIdentityCutoverError {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub owner_subject: String,
    pub is_public: bool,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageVersion {
    pub id: String,
    pub package_id: String,
    pub version: String,
    pub checksum_sha256: String,
    pub storage_key: String,
    pub size_bytes: u64,
    pub is_yanked: bool,
    pub published_at_unix_seconds: i64,
    pub yanked_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPackage {
    pub id: String,
    pub name: String,
    pub owner_subject: String,
    pub is_public: bool,
    pub now_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishVersion {
    pub id: String,
    pub package_id: String,
    pub version: String,
    pub checksum_sha256: String,
    pub storage_key: String,
    pub size_bytes: u64,
    pub now_unix_seconds: i64,
}

/// One immutable package-version reservation in a workspace publication.
/// All reservations are committed together or none become durable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePublishReservation {
    pub package: NewPackage,
    pub version_id: String,
    pub version: String,
    pub checksum_sha256: String,
    pub storage_key: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePublishOutcome {
    pub package: Package,
    pub version: PublishOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    Created(PackageVersion),
    AlreadyExists(PackageVersion),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidPackageName,
    InvalidAuthHubSubject,
    InvalidVersion,
    InvalidChecksum,
    PackageAlreadyExists,
    PackageOwnershipConflict,
    PackageNotFound,
    VersionImmutable,
    VersionNotFound,
    VersionAlreadyYanked,
    VersionNotYanked,
    InvalidIdentifier,
    Database(String),
}

pub trait PackageRepository {
    fn create_package(&mut self, request: NewPackage) -> Result<Package, StoreError>;
    fn find_package(&self, name: &str) -> Option<&Package>;
    /// Deletes a package and returns its versions so the caller can remove the
    /// corresponding artifact objects only after the registry mutation commits.
    fn delete_package(&mut self, name: &str) -> Result<Vec<PackageVersion>, StoreError>;
    fn publish_version(&mut self, request: PublishVersion) -> Result<PublishOutcome, StoreError>;
    fn find_version(&self, package_id: &str, version: &str) -> Option<&PackageVersion>;
    fn set_yanked(
        &mut self,
        package_id: &str,
        version: &str,
        yanked: bool,
        now_unix_seconds: i64,
    ) -> Result<PackageVersion, StoreError>;
}

/// Async counterpart to [`PackageRepository`] for networked persistence.
///
/// The synchronous trait remains the stable boundary used by the in-memory
/// server implementation. SQLx must not be hidden behind `block_on`, because
/// that can deadlock request executors. Route integration should depend on this
/// trait once the server state is made async.
#[async_trait]
pub trait AsyncPackageRepository: Send + Sync {
    async fn create_package(&self, request: NewPackage) -> Result<Package, StoreError>;
    async fn find_package(&self, name: &str) -> Result<Option<Package>, StoreError>;
    /// Deletes the package transactionally and returns storage records for
    /// post-commit artifact cleanup.
    async fn delete_package(&self, name: &str) -> Result<Vec<PackageVersion>, StoreError>;
    async fn publish_version(&self, request: PublishVersion) -> Result<PublishOutcome, StoreError>;
    async fn find_version(&self, package_id: &str, version: &str) -> Result<Option<PackageVersion>, StoreError>;
    async fn set_yanked(
        &self,
        package_id: &str,
        version: &str,
        yanked: bool,
        now_unix_seconds: i64,
    ) -> Result<PackageVersion, StoreError>;
}

/// PostgreSQL implementation of the package repository contract.
///
/// `publish_version` and `set_yanked` use transactions and row locks so the
/// checksum-idempotency and state-transition rules hold under concurrent calls.
#[derive(Clone, Debug)]
pub struct SqlxPackageRepository {
    pool: PgPool,
}

impl SqlxPackageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Applies the registry-owned migrations. Legacy data import intentionally
    /// remains separate because it requires a reviewed subject mapping.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        for (_, migration) in migrations::ALL {
            if migration.lines().all(|line| line.trim().is_empty() || line.trim_start().starts_with("--")) {
                continue;
            }
            sqlx::raw_sql(migration).execute(&self.pool).await.map_err(database_error)?;
        }
        Ok(())
    }

    /// Reserves every package/version in one PostgreSQL transaction. Artifact
    /// bytes are staged by the caller before this method and are compensated
    /// if this transaction fails; this boundary guarantees that a malformed
    /// later workspace member cannot leave earlier registry metadata behind.
    pub async fn publish_workspace_batch(
        &self,
        reservations: &[WorkspacePublishReservation],
    ) -> Result<Vec<WorkspacePublishOutcome>, StoreError> {
        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        let mut outcomes = Vec::with_capacity(reservations.len());
        for reservation in reservations {
            validate_package_name(&reservation.package.name)?;
            validate_subject(&reservation.package.owner_subject)?;
            validate_version(&reservation.version)?;
            validate_checksum(&reservation.checksum_sha256)?;
            let requested_package_id = parse_identifier(&reservation.package.id)?;
            let version_id = parse_identifier(&reservation.version_id)?;
            let created_at = timestamp(reservation.package.now_unix_seconds)?;
            let published_at = timestamp(reservation.package.now_unix_seconds)?;
            let package = match sqlx::query_as::<_, PackageRow>(
                "SELECT id, name, owner_subject, is_public, created_at_utc, updated_at_utc \
                 FROM pckg_packages WHERE name = $1 FOR UPDATE",
            )
            .bind(&reservation.package.name)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(database_error)?
            {
                Some(row) => row.into_domain(),
                None => {
                    let inserted = sqlx::query_as::<_, PackageRow>(
                        "INSERT INTO pckg_packages (id, name, owner_subject, is_public, created_at_utc, updated_at_utc) \
                         VALUES ($1, $2, $3, $4, $5, $5) ON CONFLICT (name) DO NOTHING \
                         RETURNING id, name, owner_subject, is_public, created_at_utc, updated_at_utc",
                    )
                    .bind(requested_package_id)
                    .bind(&reservation.package.name)
                    .bind(&reservation.package.owner_subject)
                    .bind(reservation.package.is_public)
                    .bind(created_at)
                    .fetch_optional(&mut *transaction)
                    .await
                    .map_err(database_error)?;
                    match inserted {
                        Some(row) => row.into_domain(),
                        None => sqlx::query_as::<_, PackageRow>(
                            "SELECT id, name, owner_subject, is_public, created_at_utc, updated_at_utc \
                             FROM pckg_packages WHERE name = $1 FOR UPDATE",
                        )
                        .bind(&reservation.package.name)
                        .fetch_one(&mut *transaction)
                        .await
                        .map_err(database_error)?
                        .into_domain(),
                    }
                }
            };
            if package.owner_subject != reservation.package.owner_subject {
                return Err(StoreError::PackageOwnershipConflict);
            }
            let checksum = reservation.checksum_sha256.to_ascii_lowercase();
            let outcome = if let Some(existing) =
                find_version_in_transaction(&mut transaction, parse_identifier(&package.id)?, &reservation.version)
                    .await?
            {
                if existing.checksum_sha256.eq_ignore_ascii_case(&checksum) {
                    PublishOutcome::AlreadyExists(existing)
                } else {
                    return Err(StoreError::VersionImmutable);
                }
            } else {
                let package_id = parse_identifier(&package.id)?;
                let inserted = sqlx::query_as::<_, PackageVersionRow>(
                    "INSERT INTO pckg_package_versions \
                     (id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc) \
                     VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7, NULL) \
                     ON CONFLICT (package_id, version) DO NOTHING \
                     RETURNING id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc",
                )
                .bind(version_id)
                .bind(package_id)
                .bind(&reservation.version)
                .bind(&checksum)
                .bind(&reservation.storage_key)
                .bind(i64::try_from(reservation.size_bytes).map_err(|_| StoreError::InvalidIdentifier)?)
                .bind(published_at)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(database_error)?;
                match inserted {
                    Some(row) => PublishOutcome::Created(row.into_domain()),
                    None => {
                        let existing = find_version_in_transaction(&mut transaction, package_id, &reservation.version)
                            .await?
                            .ok_or_else(|| StoreError::Database("version conflict row disappeared".into()))?;
                        if existing.checksum_sha256.eq_ignore_ascii_case(&checksum) {
                            PublishOutcome::AlreadyExists(existing)
                        } else {
                            return Err(StoreError::VersionImmutable);
                        }
                    }
                }
            };
            outcomes.push(WorkspacePublishOutcome { package, version: outcome });
        }
        transaction.commit().await.map_err(database_error)?;
        Ok(outcomes)
    }

    /// Imports legacy package ownership only after every owner has an explicit,
    /// reviewed GitHub subject mapping. The method never reads `AspNetUsers` or
    /// uses a username/email-based join. It first writes a durable audit run;
    /// if even one package owner is missing, it commits the rejection report and
    /// imports neither packages nor versions.
    ///
    /// The legacy application tables are intentionally addressed with their
    /// exact EF/PostgreSQL names: `"Packages"` and `"PackageVersions"`. A
    /// deployment runner must call [`Self::migrate`] before invoking this method.
    pub async fn import_legacy_identity_cutover(
        &self,
        request: LegacyIdentityCutoverRequest,
    ) -> Result<LegacyIdentityCutoverReport, LegacyIdentityCutoverError> {
        validate_cutover_request(&request)?;
        let run_uuid = parse_identifier(&request.run_id).map_err(LegacyIdentityCutoverError::from)?;
        let started_at = timestamp(request.now_unix_seconds).map_err(LegacyIdentityCutoverError::from)?;
        let mut transaction = self.pool.begin().await.map_err(database_error)?;

        sqlx::query(
            "INSERT INTO pckg_legacy_identity_cutover_runs \
             (run_id, requested_by, started_at_utc, status) VALUES ($1, $2, $3, 'running')",
        )
        .bind(run_uuid)
        .bind(&request.requested_by)
        .bind(started_at)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;

        for mapping in &request.mappings {
            let inserted = sqlx::query_scalar::<_, String>(
                "INSERT INTO pckg_legacy_identity_subject_map \
                 (legacy_identity_id, auth_hub_subject, approved_by, approved_at_utc) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (legacy_identity_id) DO NOTHING \
                 RETURNING auth_hub_subject",
            )
            .bind(&mapping.legacy_identity_id)
            .bind(&mapping.github_subject)
            .bind(&mapping.approved_by)
            .bind(timestamp(mapping.approved_at_unix_seconds).map_err(LegacyIdentityCutoverError::from)?)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(database_error)?;
            if inserted.is_none() {
                let existing = sqlx::query_scalar::<_, String>(
                    "SELECT auth_hub_subject FROM pckg_legacy_identity_subject_map \
                     WHERE legacy_identity_id = $1 FOR KEY SHARE",
                )
                .bind(&mapping.legacy_identity_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(database_error)?;
                if existing != mapping.github_subject {
                    return Err(LegacyIdentityCutoverError::InvalidRequest(format!(
                        "legacy identity `{}` is already approved for a different GitHub subject",
                        mapping.legacy_identity_id
                    )));
                }
            }
        }

        let legacy_package_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM \"Packages\"")
            .fetch_one(&mut *transaction)
            .await
            .map_err(database_error)?;
        let unmapped_rows = sqlx::query_as::<_, UnmappedLegacyIdentityRow>(
            "SELECT p.\"OwnerUserId\" AS legacy_identity_id, COUNT(*)::BIGINT AS package_count \
             FROM \"Packages\" AS p \
             LEFT JOIN pckg_legacy_identity_subject_map AS m \
               ON m.legacy_identity_id = p.\"OwnerUserId\" \
             WHERE m.legacy_identity_id IS NULL \
             GROUP BY p.\"OwnerUserId\" \
             ORDER BY p.\"OwnerUserId\"",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(database_error)?;
        let unmapped_identities = unmapped_rows
            .into_iter()
            .map(UnmappedLegacyIdentityRow::into_domain)
            .collect::<Result<Vec<_>, _>>()
            .map_err(LegacyIdentityCutoverError::from)?;

        if !unmapped_identities.is_empty() {
            for unmapped in &unmapped_identities {
                sqlx::query(
                    "INSERT INTO pckg_legacy_identity_cutover_unmapped_identities \
                     (run_id, legacy_identity_id, package_count) VALUES ($1, $2, $3)",
                )
                .bind(run_uuid)
                .bind(&unmapped.legacy_identity_id)
                .bind(i64::try_from(unmapped.package_count).map_err(|_| StoreError::InvalidIdentifier)?)
                .execute(&mut *transaction)
                .await
                .map_err(database_error)?;
            }
            let report = LegacyIdentityCutoverReport {
                run_id: request.run_id,
                status: LegacyIdentityCutoverStatus::RejectedUnmappedIdentity,
                mapped_identity_count: request.mappings.len() as u64,
                legacy_package_count: as_u64(legacy_package_count)?,
                imported_package_count: 0,
                imported_version_count: 0,
                unmapped_identities,
            };
            write_cutover_report(&mut transaction, run_uuid, &report, started_at).await?;
            transaction.commit().await.map_err(database_error)?;
            return Err(LegacyIdentityCutoverError::RejectedUnmappedIdentities(report));
        }

        let imported_package_count = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pckg_packages \
             (id, name, owner_subject, is_public, created_at_utc, updated_at_utc) \
             SELECT p.\"Id\", p.\"Name\", m.auth_hub_subject, p.\"IsPublic\", p.\"CreatedAtUtc\", p.\"UpdatedAtUtc\" \
             FROM \"Packages\" AS p \
             INNER JOIN pckg_legacy_identity_subject_map AS m \
               ON m.legacy_identity_id = p.\"OwnerUserId\" \
             ON CONFLICT (id) DO NOTHING \
             RETURNING id",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(database_error)?
        .len() as u64;
        let imported_version_count = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO pckg_package_versions \
             (id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc) \
             SELECT v.\"Id\", v.\"PackageId\", v.\"Version\", LOWER(v.\"ChecksumSha256\"), \
                    v.\"StorageKey\", v.\"SizeBytes\", v.\"IsYanked\", v.\"PublishedAtUtc\", v.\"YankedAtUtc\" \
             FROM \"PackageVersions\" AS v \
             INNER JOIN pckg_packages AS p ON p.id = v.\"PackageId\" \
             ON CONFLICT (id) DO NOTHING \
             RETURNING id",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(database_error)?
        .len() as u64;
        let report = LegacyIdentityCutoverReport {
            run_id: request.run_id,
            status: LegacyIdentityCutoverStatus::Completed,
            mapped_identity_count: request.mappings.len() as u64,
            legacy_package_count: as_u64(legacy_package_count)?,
            imported_package_count,
            imported_version_count,
            unmapped_identities: Vec::new(),
        };
        write_cutover_report(&mut transaction, run_uuid, &report, started_at).await?;
        transaction.commit().await.map_err(database_error)?;
        Ok(report)
    }
}

impl SqlxPackageRepository {
    /// Creates the API-key table after the package registry migration. Kept
    /// explicit so test fixtures can opt into only the surface they exercise.
    pub async fn migrate_api_keys(&self) -> Result<(), ApiKeyStoreError> {
        sqlx::raw_sql(migrations::CREATE_API_KEYS).execute(&self.pool).await.map_err(api_key_database_error)?;
        Ok(())
    }

    /// Applies registry-owned administration tables. It deliberately creates
    /// no role rows, so a deployment cannot accidentally bootstrap privilege.
    pub async fn migrate_administration(&self) -> Result<(), AdministrationStoreError> {
        sqlx::raw_sql(migrations::CREATE_ADMINISTRATION)
            .execute(&self.pool)
            .await
            .map_err(administration_database_error)?;
        Ok(())
    }

    /// Applies the independent operations tables.  This stays separate from
    /// role management so focused fixtures can opt into the exact durable
    /// boundary they exercise.
    pub async fn migrate_registry_operations(&self) -> Result<(), RegistryOperationsStoreError> {
        sqlx::raw_sql(migrations::CREATE_REGISTRY_OPERATIONS)
            .execute(&self.pool)
            .await
            .map_err(registry_operations_database_error)?;
        Ok(())
    }

    /// Seeds exactly one initial SuperAdmin only when the complete role table
    /// is empty. The caller must supply an explicit `github:<numeric-id>` from
    /// deployment configuration; a later config change cannot elevate another
    /// subject once any administrator has been recorded.
    pub async fn bootstrap_super_admin(
        &self,
        subject: &str,
        now_unix_seconds: i64,
    ) -> Result<bool, AdministrationStoreError> {
        validate_administration_subject(subject)?;
        let timestamp = DateTime::from_timestamp(now_unix_seconds, 0).ok_or(AdministrationStoreError::InvalidRole)?;
        let inserted = sqlx::query("INSERT INTO pckg_admin_roles (subject,role,granted_by_subject,granted_at_utc) SELECT $1,'superadmin',$1,$2 WHERE NOT EXISTS (SELECT 1 FROM pckg_admin_roles)")
            .bind(subject).bind(timestamp).execute(&self.pool).await.map_err(administration_database_error)?;
        Ok(inserted.rows_affected() == 1)
    }
}

#[async_trait]
impl AsyncRegistryOperationsRepository for SqlxPackageRepository {
    async fn list_blocked_link_policies(&self) -> Result<Vec<BlockedLinkPolicy>, RegistryOperationsStoreError> {
        let rows = sqlx::query_as::<_, BlockedLinkPolicyRow>(
            "SELECT id,pattern,note,created_by_subject,created_at_utc \
             FROM pckg_blocked_link_patterns ORDER BY created_at_utc DESC,id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(registry_operations_database_error)?;
        Ok(rows.into_iter().map(BlockedLinkPolicyRow::into_domain).collect())
    }

    async fn add_blocked_link_policy(
        &self,
        policy: NewBlockedLinkPolicy,
    ) -> Result<BlockedLinkPolicy, RegistryOperationsStoreError> {
        validate_registry_operations_subject(&policy.created_by_subject)?;
        let pattern = BlockedLinkPattern::new(&policy.pattern)
            .map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkPattern)?;
        let id = Uuid::parse_str(&policy.id).map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkId)?;
        let at = registry_operations_timestamp(policy.created_at_unix_seconds)?;
        let note = normalize_operations_note(policy.note)?;
        let row = sqlx::query_as::<_, BlockedLinkPolicyRow>(
            "INSERT INTO pckg_blocked_link_patterns \
             (id,pattern,note,created_by_subject,created_at_utc) VALUES ($1,$2,$3,$4,$5) \
             RETURNING id,pattern,note,created_by_subject,created_at_utc",
        )
        .bind(id)
        .bind(pattern.as_str())
        .bind(note)
        .bind(policy.created_by_subject)
        .bind(at)
        .fetch_one(&self.pool)
        .await
        .map_err(registry_operations_insert_error)?;
        Ok(row.into_domain())
    }

    async fn delete_blocked_link_policy(&self, id: &str) -> Result<(), RegistryOperationsStoreError> {
        let id = Uuid::parse_str(id).map_err(|_| RegistryOperationsStoreError::InvalidBlockedLinkId)?;
        let deleted = sqlx::query("DELETE FROM pckg_blocked_link_patterns WHERE id=$1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(registry_operations_database_error)?;
        if deleted.rows_affected() == 0 {
            return Err(RegistryOperationsStoreError::NotFound);
        }
        Ok(())
    }

    async fn append_registry_activity(
        &self,
        activity: NewRegistryActivity,
    ) -> Result<RegistryActivity, RegistryOperationsStoreError> {
        validate_registry_activity(&activity)?;
        let at = registry_operations_timestamp(activity.occurred_at_unix_seconds)?;
        let mut transaction = self.pool.begin().await.map_err(registry_operations_database_error)?;
        let row = sqlx::query_as::<_, RegistryActivityRow>(
            "INSERT INTO pckg_registry_activity \
             (occurred_at_utc,severity,action,message,trace_id,actor_subject,package_name,version) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
             RETURNING id,occurred_at_utc,severity,action,message,trace_id,actor_subject,package_name,version",
        )
        .bind(at)
        .bind(activity.severity)
        .bind(activity.action)
        .bind(activity.message)
        .bind(activity.trace_id)
        .bind(activity.actor_subject)
        .bind(activity.package_name)
        .bind(activity.version)
        .fetch_one(&mut *transaction)
        .await
        .map_err(registry_operations_database_error)?;
        // Retain the legacy 500-entry diagnostic window atomically with the append.
        sqlx::query(
            "DELETE FROM pckg_registry_activity WHERE id IN ( \
             SELECT id FROM pckg_registry_activity ORDER BY occurred_at_utc DESC,id DESC OFFSET 500)",
        )
        .execute(&mut *transaction)
        .await
        .map_err(registry_operations_database_error)?;
        transaction.commit().await.map_err(registry_operations_database_error)?;
        Ok(row.into_domain())
    }

    async fn recent_registry_activity(&self, take: u16) -> Result<Vec<RegistryActivity>, RegistryOperationsStoreError> {
        let take = i64::from(take.clamp(1, 500));
        let rows = sqlx::query_as::<_, RegistryActivityRow>(
            "SELECT id,occurred_at_utc,severity,action,message,trace_id,actor_subject,package_name,version \
             FROM pckg_registry_activity ORDER BY occurred_at_utc DESC,id DESC LIMIT $1",
        )
        .bind(take)
        .fetch_all(&self.pool)
        .await
        .map_err(registry_operations_database_error)?;
        Ok(rows.into_iter().map(RegistryActivityRow::into_domain).collect())
    }

    async fn record_weekly_spotlight(
        &self,
        run: WeeklySpotlightRun,
    ) -> Result<WeeklySpotlightRun, RegistryOperationsStoreError> {
        validate_registry_operations_subject(&run.ran_by_subject)?;
        if run.delivery != "in_app_only" {
            return Err(RegistryOperationsStoreError::InvalidWeeklySpotlightRun);
        }
        let id = Uuid::parse_str(&run.id).map_err(|_| RegistryOperationsStoreError::InvalidWeeklySpotlightRun)?;
        let at = registry_operations_timestamp(run.ran_at_unix_seconds)?;
        let activity_count =
            i64::try_from(run.activity_count).map_err(|_| RegistryOperationsStoreError::InvalidWeeklySpotlightRun)?;
        sqlx::query("INSERT INTO pckg_weekly_spotlight_runs (id,ran_by_subject,ran_at_utc,activity_count,delivery) VALUES ($1,$2,$3,$4,$5)")
            .bind(id).bind(&run.ran_by_subject).bind(at).bind(activity_count).bind(&run.delivery)
            .execute(&self.pool).await.map_err(registry_operations_database_error)?;
        Ok(run)
    }
}

#[async_trait]
impl AsyncAdministrationRepository for SqlxPackageRepository {
    async fn list_admin_roles(&self) -> Result<Vec<AdminRoleAssignment>, AdministrationStoreError> {
        let rows = sqlx::query_as::<_, AdminRoleRow>(
            "SELECT subject, role, granted_by_subject, granted_at_utc FROM pckg_admin_roles ORDER BY subject, role",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(administration_database_error)?;
        rows.into_iter().map(AdminRoleRow::into_domain).collect()
    }

    async fn roles_for_subject(&self, subject: &str) -> Result<Vec<AdminRole>, AdministrationStoreError> {
        validate_administration_subject(subject)?;
        let roles = sqlx::query_scalar::<_, String>("SELECT role FROM pckg_admin_roles WHERE subject=$1")
            .bind(subject)
            .fetch_all(&self.pool)
            .await
            .map_err(administration_database_error)?;
        Ok(roles.into_iter().filter_map(|role| AdminRole::from_stored(&role)).collect())
    }

    async fn grant_admin_role(&self, assignment: AdminRoleAssignment) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&assignment.subject)?;
        validate_administration_subject(&assignment.granted_by_subject)?;
        let Some(role) = assignment.role.stored() else {
            return Err(AdministrationStoreError::InvalidRole);
        };
        let timestamp = DateTime::from_timestamp(assignment.granted_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidRole)?;
        sqlx::query("INSERT INTO pckg_admin_roles (subject,role,granted_by_subject,granted_at_utc) VALUES ($1,$2,$3,$4) ON CONFLICT (subject,role) DO NOTHING")
            .bind(assignment.subject).bind(role).bind(assignment.granted_by_subject).bind(timestamp)
            .execute(&self.pool).await.map_err(administration_database_error)?;
        Ok(())
    }
    async fn replace_admin_roles(
        &self,
        subject: &str,
        roles: Vec<AdminRole>,
        granted_by_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(subject)?;
        validate_administration_subject(granted_by_subject)?;
        if roles.iter().any(|role| role.stored().is_none()) {
            return Err(AdministrationStoreError::InvalidRole);
        }
        let timestamp = DateTime::from_timestamp(now_unix_seconds, 0).ok_or(AdministrationStoreError::InvalidRole)?;
        let mut transaction = self.pool.begin().await.map_err(administration_database_error)?;
        sqlx::query("DELETE FROM pckg_admin_roles WHERE subject=$1")
            .bind(subject)
            .execute(&mut *transaction)
            .await
            .map_err(administration_database_error)?;
        for role in roles {
            sqlx::query(
                "INSERT INTO pckg_admin_roles (subject,role,granted_by_subject,granted_at_utc) VALUES ($1,$2,$3,$4)",
            )
            .bind(subject)
            .bind(role.stored().expect("validated role"))
            .bind(granted_by_subject)
            .bind(timestamp)
            .execute(&mut *transaction)
            .await
            .map_err(administration_database_error)?;
        }
        transaction.commit().await.map_err(administration_database_error)?;
        Ok(())
    }

    async fn set_publisher_verification(
        &self,
        verification: PublisherVerification,
    ) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&verification.subject)?;
        validate_administration_subject(&verification.reviewed_by_subject)?;
        let timestamp = DateTime::from_timestamp(verification.reviewed_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidDecision)?;
        sqlx::query("INSERT INTO pckg_publisher_verifications (subject,is_verified,reviewed_by_subject,reviewed_at_utc) VALUES ($1,$2,$3,$4) ON CONFLICT (subject) DO UPDATE SET is_verified=EXCLUDED.is_verified, reviewed_by_subject=EXCLUDED.reviewed_by_subject, reviewed_at_utc=EXCLUDED.reviewed_at_utc")
            .bind(verification.subject).bind(verification.is_verified).bind(verification.reviewed_by_subject).bind(timestamp)
            .execute(&self.pool).await.map_err(administration_database_error)?;
        Ok(())
    }

    async fn get_publisher_verification(
        &self,
        subject: &str,
    ) -> Result<Option<PublisherVerification>, AdministrationStoreError> {
        validate_administration_subject(subject)?;
        sqlx::query_as::<_, PublisherVerificationRow>("SELECT subject,is_verified,reviewed_by_subject,reviewed_at_utc FROM pckg_publisher_verifications WHERE subject=$1")
            .bind(subject).fetch_optional(&self.pool).await.map_err(administration_database_error)?
            .map(PublisherVerificationRow::into_domain).transpose()
    }
    async fn list_publisher_verifications(&self) -> Result<Vec<PublisherVerification>, AdministrationStoreError> {
        let rows = sqlx::query_as::<_, PublisherVerificationRow>("SELECT subject,is_verified,reviewed_by_subject,reviewed_at_utc FROM pckg_publisher_verifications ORDER BY subject").fetch_all(&self.pool).await.map_err(administration_database_error)?;
        rows.into_iter().map(PublisherVerificationRow::into_domain).collect()
    }

    async fn grant_resource_permission(&self, grant: ResourcePermissionGrant) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&grant.subject)?;
        validate_administration_subject(&grant.granted_by_subject)?;
        validate_resource(&grant.resource_kind, &grant.resource_id, &grant.capability)?;
        let timestamp = DateTime::from_timestamp(grant.granted_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidResource)?;
        sqlx::query("INSERT INTO pckg_resource_permissions (subject,resource_kind,resource_id,capability,granted_by_subject,granted_at_utc) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (subject,resource_kind,resource_id,capability) DO NOTHING")
            .bind(grant.subject).bind(grant.resource_kind).bind(grant.resource_id).bind(grant.capability).bind(grant.granted_by_subject).bind(timestamp)
            .execute(&self.pool).await.map_err(administration_database_error)?;
        Ok(())
    }

    async fn list_resource_permissions(
        &self,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError> {
        validate_resource(resource_kind, resource_id, "moderate")?;
        let rows = sqlx::query_as::<_, ResourcePermissionRow>("SELECT subject,resource_kind,resource_id,capability,granted_by_subject,granted_at_utc FROM pckg_resource_permissions WHERE resource_kind=$1 AND resource_id=$2 ORDER BY subject")
            .bind(resource_kind).bind(resource_id).fetch_all(&self.pool).await.map_err(administration_database_error)?;
        rows.into_iter().map(ResourcePermissionRow::into_domain).collect()
    }

    async fn list_all_resource_permissions(&self) -> Result<Vec<ResourcePermissionGrant>, AdministrationStoreError> {
        let rows = sqlx::query_as::<_, ResourcePermissionRow>("SELECT subject,resource_kind,resource_id,capability,granted_by_subject,granted_at_utc FROM pckg_resource_permissions ORDER BY resource_kind,resource_id,subject")
            .fetch_all(&self.pool).await.map_err(administration_database_error)?;
        rows.into_iter().map(ResourcePermissionRow::into_domain).collect()
    }

    async fn record_package_review(&self, decision: PackageReviewDecision) -> Result<(), AdministrationStoreError> {
        validate_administration_subject(&decision.decided_by_subject)?;
        if !matches!(decision.decision.as_str(), "approved" | "rejected" | "yanked" | "unyanked")
            || decision.reason.len() > 4000
        {
            return Err(AdministrationStoreError::InvalidDecision);
        }
        let package_id =
            Uuid::parse_str(&decision.package_id).map_err(|_| AdministrationStoreError::InvalidPackageId)?;
        let timestamp = DateTime::from_timestamp(decision.decided_at_unix_seconds, 0)
            .ok_or(AdministrationStoreError::InvalidDecision)?;
        sqlx::query("INSERT INTO pckg_package_review_decisions (package_id,version,decision,reason,decided_by_subject,decided_at_utc) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(package_id).bind(decision.version).bind(decision.decision).bind(decision.reason).bind(decision.decided_by_subject).bind(timestamp)
            .execute(&self.pool).await.map_err(administration_database_error)?;
        Ok(())
    }
}

#[async_trait]
impl AsyncPackageReviewRepository for SqlxPackageRepository {
    async fn submit_package_review(
        &self,
        review: PackageReviewRequest,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError> {
        validate_review_request(&review)?;
        let id = Uuid::parse_str(&review.id).map_err(|_| PackageReviewQueueError::InvalidReviewId)?;
        let package_id = Uuid::parse_str(&review.package_id).map_err(|_| PackageReviewQueueError::InvalidPackageId)?;
        let submitted_at = DateTime::from_timestamp(review.submitted_at_unix_seconds, 0)
            .ok_or(PackageReviewQueueError::InvalidReason)?;
        let row = sqlx::query_as::<_, PackageReviewRequestRow>(
            "INSERT INTO pckg_package_review_requests \
             (id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc) \
             VALUES ($1,$2,$3,$4,'pending',$5,NULL,NULL,NULL) \
             RETURNING id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc",
        )
        .bind(id)
        .bind(package_id)
        .bind(&review.requested_by_subject)
        .bind(&review.reason)
        .bind(submitted_at)
        .fetch_one(&self.pool)
        .await
        .map_err(review_queue_database_error)?;
        row.into_domain()
    }

    async fn list_package_reviews(&self) -> Result<Vec<PackageReviewRequest>, PackageReviewQueueError> {
        let rows = sqlx::query_as::<_, PackageReviewRequestRow>(
            "SELECT id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc \
             FROM pckg_package_review_requests ORDER BY submitted_at_utc DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(review_queue_database_error)?;
        rows.into_iter().map(PackageReviewRequestRow::into_domain).collect()
    }

    async fn action_package_review(
        &self,
        review_id: &str,
        action: &str,
        reviewer_subject: &str,
        notes: Option<String>,
        reviewed_at_unix_seconds: i64,
    ) -> Result<PackageReviewRequest, PackageReviewQueueError> {
        let id = Uuid::parse_str(review_id).map_err(|_| PackageReviewQueueError::InvalidReviewId)?;
        validate_subject_for_review(reviewer_subject)?;
        let action = normalize_review_action(action)?;
        let notes = normalize_review_notes(notes)?;
        let reviewed_at =
            DateTime::from_timestamp(reviewed_at_unix_seconds, 0).ok_or(PackageReviewQueueError::InvalidAction)?;
        let row = sqlx::query_as::<_, PackageReviewRequestRow>(
            "UPDATE pckg_package_review_requests \
             SET status=$2, reviewer_subject=$3, review_notes=$4, reviewed_at_utc=$5 \
             WHERE id=$1 \
             RETURNING id,package_id,requested_by_subject,reason,status,submitted_at_utc,reviewer_subject,review_notes,reviewed_at_utc",
        )
        .bind(id)
        .bind(action)
        .bind(reviewer_subject)
        .bind(notes)
        .bind(reviewed_at)
        .fetch_optional(&self.pool)
        .await
        .map_err(review_queue_database_error)?
        .ok_or(PackageReviewQueueError::NotFound)?;
        row.into_domain()
    }
}

#[async_trait]
impl AsyncPackageCommunityReviewRepository for SqlxPackageRepository {
    async fn upsert_package_community_review(
        &self,
        review: PackageCommunityReview,
    ) -> Result<PackageCommunityReview, PackageCommunityReviewError> {
        let id = Uuid::parse_str(&review.id).map_err(|_| PackageCommunityReviewError::InvalidPackageId)?;
        let package_id =
            Uuid::parse_str(&review.package_id).map_err(|_| PackageCommunityReviewError::InvalidPackageId)?;
        validate_subject(&review.author_subject).map_err(|_| PackageCommunityReviewError::InvalidAuthHubSubject)?;
        if !(1..=5).contains(&review.rating) {
            return Err(PackageCommunityReviewError::InvalidRating);
        }
        if review.comment.trim().is_empty() {
            return Err(PackageCommunityReviewError::InvalidComment);
        }
        let created = DateTime::from_timestamp(review.created_at_unix_seconds, 0)
            .ok_or(PackageCommunityReviewError::InvalidComment)?;
        let updated = DateTime::from_timestamp(review.updated_at_unix_seconds, 0)
            .ok_or(PackageCommunityReviewError::InvalidComment)?;
        let row = sqlx::query_as::<_, PackageCommunityReviewRow>(
            "INSERT INTO pckg_package_community_reviews (id,package_id,author_subject,rating,comment,created_at_utc,updated_at_utc) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (package_id,author_subject) DO UPDATE SET rating=EXCLUDED.rating,comment=EXCLUDED.comment,updated_at_utc=EXCLUDED.updated_at_utc RETURNING id,package_id,author_subject,rating,comment,created_at_utc,updated_at_utc"
        ).bind(id).bind(package_id).bind(&review.author_subject).bind(review.rating).bind(&review.comment).bind(created).bind(updated).fetch_one(&self.pool).await.map_err(|error| PackageCommunityReviewError::Database(error.to_string()))?;
        row.into_domain()
    }
    async fn list_package_community_reviews(
        &self,
        package_id: &str,
    ) -> Result<Vec<PackageCommunityReview>, PackageCommunityReviewError> {
        let package_id = Uuid::parse_str(package_id).map_err(|_| PackageCommunityReviewError::InvalidPackageId)?;
        let rows = sqlx::query_as::<_, PackageCommunityReviewRow>("SELECT id,package_id,author_subject,rating,comment,created_at_utc,updated_at_utc FROM pckg_package_community_reviews WHERE package_id=$1 ORDER BY created_at_utc DESC,id DESC").bind(package_id).fetch_all(&self.pool).await.map_err(|error| PackageCommunityReviewError::Database(error.to_string()))?;
        rows.into_iter().map(PackageCommunityReviewRow::into_domain).collect()
    }
}

#[derive(sqlx::FromRow)]
struct PackageCommunityReviewRow {
    id: Uuid,
    package_id: Uuid,
    author_subject: String,
    rating: i16,
    comment: String,
    created_at_utc: chrono::DateTime<chrono::Utc>,
    updated_at_utc: chrono::DateTime<chrono::Utc>,
}
impl PackageCommunityReviewRow {
    fn into_domain(self) -> Result<PackageCommunityReview, PackageCommunityReviewError> {
        Ok(PackageCommunityReview {
            id: self.id.to_string(),
            package_id: self.package_id.to_string(),
            author_subject: self.author_subject,
            rating: self.rating,
            comment: self.comment,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        })
    }
}

#[derive(sqlx::FromRow)]
struct PackageReviewRequestRow {
    id: Uuid,
    package_id: Uuid,
    requested_by_subject: String,
    reason: String,
    status: String,
    submitted_at_utc: DateTime<Utc>,
    reviewer_subject: Option<String>,
    review_notes: Option<String>,
    reviewed_at_utc: Option<DateTime<Utc>>,
}

impl PackageReviewRequestRow {
    fn into_domain(self) -> Result<PackageReviewRequest, PackageReviewQueueError> {
        validate_subject_for_review(&self.requested_by_subject)?;
        if let Some(subject) = &self.reviewer_subject {
            validate_subject_for_review(subject)?;
        }
        normalize_review_action(&self.status)?;
        Ok(PackageReviewRequest {
            id: self.id.to_string(),
            package_id: self.package_id.to_string(),
            requested_by_subject: self.requested_by_subject,
            reason: self.reason,
            status: self.status,
            submitted_at_unix_seconds: self.submitted_at_utc.timestamp(),
            reviewer_subject: self.reviewer_subject,
            review_notes: self.review_notes,
            reviewed_at_unix_seconds: self.reviewed_at_utc.map(|time| time.timestamp()),
        })
    }
}

fn validate_review_request(review: &PackageReviewRequest) -> Result<(), PackageReviewQueueError> {
    Uuid::parse_str(&review.id).map_err(|_| PackageReviewQueueError::InvalidReviewId)?;
    Uuid::parse_str(&review.package_id).map_err(|_| PackageReviewQueueError::InvalidPackageId)?;
    validate_subject_for_review(&review.requested_by_subject)?;
    if review.reason.trim().is_empty() || review.reason.trim() != review.reason || review.reason.len() > 4000 {
        return Err(PackageReviewQueueError::InvalidReason);
    }
    Ok(())
}

fn validate_subject_for_review(subject: &str) -> Result<(), PackageReviewQueueError> {
    validate_subject(subject).map_err(|_| PackageReviewQueueError::InvalidAuthHubSubject)
}

fn normalize_review_action(action: &str) -> Result<&'static str, PackageReviewQueueError> {
    let normalized = action.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "approved" => Ok("approved"),
        "needs_changes" | "needschanges" => Ok("needs_changes"),
        "rejected" => Ok("rejected"),
        _ => Err(PackageReviewQueueError::InvalidAction),
    }
}

fn normalize_review_notes(notes: Option<String>) -> Result<Option<String>, PackageReviewQueueError> {
    let notes = notes.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()));
    if notes.as_ref().is_some_and(|value| value.len() > 4000) {
        return Err(PackageReviewQueueError::InvalidAction);
    }
    Ok(notes)
}

fn review_queue_database_error(error: sqlx::Error) -> PackageReviewQueueError {
    PackageReviewQueueError::Database(error.to_string())
}

#[derive(sqlx::FromRow)]
struct AdminRoleRow {
    subject: String,
    role: String,
    granted_by_subject: String,
    granted_at_utc: DateTime<Utc>,
}
impl AdminRoleRow {
    fn into_domain(self) -> Result<AdminRoleAssignment, AdministrationStoreError> {
        Ok(AdminRoleAssignment {
            subject: self.subject,
            role: AdminRole::from_stored(&self.role).ok_or(AdministrationStoreError::InvalidRole)?,
            granted_by_subject: self.granted_by_subject,
            granted_at_unix_seconds: self.granted_at_utc.timestamp(),
        })
    }
}
#[derive(sqlx::FromRow)]
struct PublisherVerificationRow {
    subject: String,
    is_verified: bool,
    reviewed_by_subject: String,
    reviewed_at_utc: DateTime<Utc>,
}
impl PublisherVerificationRow {
    fn into_domain(self) -> Result<PublisherVerification, AdministrationStoreError> {
        Ok(PublisherVerification {
            subject: self.subject,
            is_verified: self.is_verified,
            reviewed_by_subject: self.reviewed_by_subject,
            reviewed_at_unix_seconds: self.reviewed_at_utc.timestamp(),
        })
    }
}
#[derive(sqlx::FromRow)]
struct ResourcePermissionRow {
    subject: String,
    resource_kind: String,
    resource_id: String,
    capability: String,
    granted_by_subject: String,
    granted_at_utc: DateTime<Utc>,
}
impl ResourcePermissionRow {
    fn into_domain(self) -> Result<ResourcePermissionGrant, AdministrationStoreError> {
        validate_resource(&self.resource_kind, &self.resource_id, &self.capability)?;
        Ok(ResourcePermissionGrant {
            subject: self.subject,
            resource_kind: self.resource_kind,
            resource_id: self.resource_id,
            capability: self.capability,
            granted_by_subject: self.granted_by_subject,
            granted_at_unix_seconds: self.granted_at_utc.timestamp(),
        })
    }
}
fn validate_administration_subject(subject: &str) -> Result<(), AdministrationStoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(AdministrationStoreError::InvalidAuthHubSubject)
}
fn validate_resource(kind: &str, id: &str, capability: &str) -> Result<(), AdministrationStoreError> {
    (matches!(kind, "package" | "board") && !id.trim().is_empty() && id.trim() == id && capability == "moderate")
        .then_some(())
        .ok_or(AdministrationStoreError::InvalidResource)
}
fn administration_database_error(error: sqlx::Error) -> AdministrationStoreError {
    AdministrationStoreError::Database(error.to_string())
}

#[derive(sqlx::FromRow)]
struct BlockedLinkPolicyRow {
    id: Uuid,
    pattern: String,
    note: Option<String>,
    created_by_subject: String,
    created_at_utc: DateTime<Utc>,
}

impl BlockedLinkPolicyRow {
    fn into_domain(self) -> BlockedLinkPolicy {
        BlockedLinkPolicy {
            id: self.id.to_string(),
            pattern: self.pattern,
            note: self.note,
            created_by_subject: self.created_by_subject,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct RegistryActivityRow {
    id: i64,
    occurred_at_utc: DateTime<Utc>,
    severity: String,
    action: String,
    message: String,
    trace_id: Option<String>,
    actor_subject: Option<String>,
    package_name: Option<String>,
    version: Option<String>,
}

impl RegistryActivityRow {
    fn into_domain(self) -> RegistryActivity {
        RegistryActivity {
            sequence: self.id,
            occurred_at_unix_seconds: self.occurred_at_utc.timestamp(),
            severity: self.severity,
            action: self.action,
            message: self.message,
            trace_id: self.trace_id,
            actor_subject: self.actor_subject,
            package_name: self.package_name,
            version: self.version,
        }
    }
}

fn validate_registry_operations_subject(subject: &str) -> Result<(), RegistryOperationsStoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(RegistryOperationsStoreError::InvalidAuthHubSubject)
}

fn registry_operations_timestamp(value: i64) -> Result<DateTime<Utc>, RegistryOperationsStoreError> {
    DateTime::from_timestamp(value, 0).ok_or(RegistryOperationsStoreError::InvalidActivity)
}

fn normalize_operations_note(note: Option<String>) -> Result<Option<String>, RegistryOperationsStoreError> {
    let note = note.and_then(|value| (!value.trim().is_empty()).then(|| value.trim().to_owned()));
    if note.as_ref().is_some_and(|value| value.len() > 2000) {
        return Err(RegistryOperationsStoreError::InvalidBlockedLinkPattern);
    }
    Ok(note)
}

fn validate_registry_activity(activity: &NewRegistryActivity) -> Result<(), RegistryOperationsStoreError> {
    if activity.severity.trim().is_empty()
        || activity.severity.len() > 64
        || activity.action.trim().is_empty()
        || activity.action.len() > 128
        || activity.message.len() > 4000
        || activity.trace_id.as_ref().is_some_and(|value| value.len() > 256)
        || activity.package_name.as_ref().is_some_and(|value| value.len() > 256)
        || activity.version.as_ref().is_some_and(|value| value.len() > 128)
    {
        return Err(RegistryOperationsStoreError::InvalidActivity);
    }
    if let Some(subject) = activity.actor_subject.as_deref() {
        validate_registry_operations_subject(subject)?;
    }
    Ok(())
}

fn registry_operations_database_error(error: sqlx::Error) -> RegistryOperationsStoreError {
    RegistryOperationsStoreError::Database(error.to_string())
}

fn registry_operations_insert_error(error: sqlx::Error) -> RegistryOperationsStoreError {
    if error.as_database_error().and_then(|database| database.code()).is_some_and(|code| code == "23505") {
        RegistryOperationsStoreError::DuplicateBlockedLinkPattern
    } else {
        registry_operations_database_error(error)
    }
}

#[async_trait]
impl AsyncApiKeyRepository for SqlxPackageRepository {
    async fn create_api_key(&self, request: NewApiKey) -> Result<ApiKey, ApiKeyStoreError> {
        validate_api_key_subject(&request.subject)?;
        validate_api_key_label(&request.label)?;
        validate_api_key_scopes(&request.scopes)?;
        if request.raw_token.trim().len() < 24 {
            return Err(ApiKeyStoreError::InvalidToken);
        }
        let id = Uuid::parse_str(&request.id).map_err(|_| ApiKeyStoreError::InvalidToken)?;
        let created_at = DateTime::from_timestamp(request.now_unix_seconds, 0).ok_or(ApiKeyStoreError::InvalidToken)?;
        let token_hash = api_key_token_hash(&request.raw_token);
        sqlx::query("INSERT INTO pckg_api_keys (id,subject,label,token_sha256,scopes,created_at_utc) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(id).bind(&request.subject).bind(&request.label).bind(token_hash).bind(&request.scopes).bind(created_at)
            .execute(&self.pool).await.map_err(api_key_database_error)?;
        Ok(ApiKey {
            id: request.id,
            subject: request.subject,
            label: request.label,
            scopes: request.scopes,
            created_at_unix_seconds: request.now_unix_seconds,
            revoked_at_unix_seconds: None,
        })
    }

    async fn list_api_keys(&self, subject: &str) -> Result<Vec<ApiKey>, ApiKeyStoreError> {
        validate_api_key_subject(subject)?;
        let rows = sqlx::query_as::<_, ApiKeyRow>("SELECT id,subject,label,scopes,created_at_utc,revoked_at_utc FROM pckg_api_keys WHERE subject=$1 ORDER BY created_at_utc DESC")
            .bind(subject).fetch_all(&self.pool).await.map_err(api_key_database_error)?;
        Ok(rows.into_iter().map(ApiKeyRow::into_domain).collect())
    }

    async fn revoke_api_key(&self, id: &str, subject: &str, now_unix_seconds: i64) -> Result<bool, ApiKeyStoreError> {
        validate_api_key_subject(subject)?;
        let id = Uuid::parse_str(id).map_err(|_| ApiKeyStoreError::InvalidToken)?;
        let revoked_at = DateTime::from_timestamp(now_unix_seconds, 0).ok_or(ApiKeyStoreError::InvalidToken)?;
        let changed = sqlx::query(
            "UPDATE pckg_api_keys SET revoked_at_utc=COALESCE(revoked_at_utc,$3) WHERE id=$1 AND subject=$2",
        )
        .bind(id)
        .bind(subject)
        .bind(revoked_at)
        .execute(&self.pool)
        .await
        .map_err(api_key_database_error)?;
        Ok(changed.rows_affected() > 0)
    }

    async fn find_active_api_key_by_token(&self, raw_token: &str) -> Result<Option<ApiKey>, ApiKeyStoreError> {
        if raw_token.trim().len() < 24 {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, ApiKeyRow>("SELECT id,subject,label,scopes,created_at_utc,revoked_at_utc FROM pckg_api_keys WHERE token_sha256=$1 AND revoked_at_utc IS NULL")
            .bind(api_key_token_hash(raw_token)).fetch_optional(&self.pool).await.map_err(api_key_database_error)?;
        Ok(row.map(ApiKeyRow::into_domain))
    }
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    subject: String,
    label: String,
    scopes: Vec<String>,
    created_at_utc: DateTime<Utc>,
    revoked_at_utc: Option<DateTime<Utc>>,
}
impl ApiKeyRow {
    fn into_domain(self) -> ApiKey {
        ApiKey {
            id: self.id.to_string(),
            subject: self.subject,
            label: self.label,
            scopes: self.scopes,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            revoked_at_unix_seconds: self.revoked_at_utc.map(|time| time.timestamp()),
        }
    }
}

fn api_key_token_hash(raw_token: &str) -> String {
    format!("{:x}", Sha256::digest(raw_token.as_bytes()))
}
fn validate_api_key_subject(subject: &str) -> Result<(), ApiKeyStoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(ApiKeyStoreError::InvalidAuthHubSubject)
}
fn validate_api_key_label(label: &str) -> Result<(), ApiKeyStoreError> {
    (!label.trim().is_empty() && label.trim() == label && label.len() <= 128)
        .then_some(())
        .ok_or(ApiKeyStoreError::InvalidLabel)
}
fn validate_api_key_scopes(scopes: &[String]) -> Result<(), ApiKeyStoreError> {
    (!scopes.is_empty() && scopes.iter().all(|scope| matches!(scope.as_str(), "read" | "publish")))
        .then_some(())
        .ok_or(ApiKeyStoreError::InvalidScope)
}
fn api_key_database_error(error: sqlx::Error) -> ApiKeyStoreError {
    ApiKeyStoreError::Database(error.to_string())
}

#[async_trait]
impl AsyncPackageRepository for SqlxPackageRepository {
    async fn create_package(&self, request: NewPackage) -> Result<Package, StoreError> {
        validate_package_name(&request.name)?;
        validate_subject(&request.owner_subject)?;
        let id = parse_identifier(&request.id)?;
        let timestamp = timestamp(request.now_unix_seconds)?;
        let result = sqlx::query(
            "INSERT INTO pckg_packages (id, name, owner_subject, is_public, created_at_utc, updated_at_utc) \
             VALUES ($1, $2, $3, $4, $5, $5)",
        )
        .bind(id)
        .bind(&request.name)
        .bind(&request.owner_subject)
        .bind(request.is_public)
        .bind(timestamp)
        .execute(&self.pool)
        .await;
        match result {
            Ok(_) => Ok(Package {
                id: request.id,
                name: request.name,
                owner_subject: request.owner_subject,
                is_public: request.is_public,
                created_at_unix_seconds: request.now_unix_seconds,
                updated_at_unix_seconds: request.now_unix_seconds,
            }),
            Err(error) if is_unique_violation(&error) => Err(StoreError::PackageAlreadyExists),
            Err(error) => Err(database_error(error)),
        }
    }

    async fn find_package(&self, name: &str) -> Result<Option<Package>, StoreError> {
        let row = sqlx::query_as::<_, PackageRow>(
            "SELECT id, name, owner_subject, is_public, created_at_utc, updated_at_utc \
             FROM pckg_packages WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(row.map(PackageRow::into_domain))
    }

    async fn delete_package(&self, name: &str) -> Result<Vec<PackageVersion>, StoreError> {
        validate_package_name(name)?;
        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        let package = sqlx::query_scalar::<_, Uuid>("SELECT id FROM pckg_packages WHERE name = $1 FOR UPDATE")
            .bind(name)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(database_error)?
            .ok_or(StoreError::PackageNotFound)?;
        let versions = sqlx::query_as::<_, PackageVersionRow>(
            "SELECT id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc \
             FROM pckg_package_versions WHERE package_id = $1 FOR UPDATE",
        )
        .bind(package)
        .fetch_all(&mut *transaction)
        .await
        .map_err(database_error)?
        .into_iter()
        .map(PackageVersionRow::into_domain)
        .collect::<Vec<_>>();
        // Review decisions retain a restrictive FK for audit integrity, so an
        // explicit package deletion removes its package-scoped audit history.
        sqlx::query("DELETE FROM pckg_package_review_decisions WHERE package_id = $1")
            .bind(package)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        sqlx::query("DELETE FROM pckg_resource_permissions WHERE resource_kind = 'package' AND resource_id = $1")
            .bind(package.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        sqlx::query("DELETE FROM pckg_package_versions WHERE package_id = $1")
            .bind(package)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        let deleted = sqlx::query("DELETE FROM pckg_packages WHERE id = $1")
            .bind(package)
            .execute(&mut *transaction)
            .await
            .map_err(database_error)?;
        if deleted.rows_affected() != 1 {
            return Err(StoreError::PackageNotFound);
        }
        transaction.commit().await.map_err(database_error)?;
        Ok(versions)
    }

    async fn publish_version(&self, request: PublishVersion) -> Result<PublishOutcome, StoreError> {
        validate_version(&request.version)?;
        validate_checksum(&request.checksum_sha256)?;
        let id = parse_identifier(&request.id)?;
        let package_id = parse_identifier(&request.package_id)?;
        let timestamp = timestamp(request.now_unix_seconds)?;
        let checksum = request.checksum_sha256.to_ascii_lowercase();
        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        let package_exists = sqlx::query_scalar::<_, Uuid>("SELECT id FROM pckg_packages WHERE id = $1 FOR KEY SHARE")
            .bind(package_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(database_error)?;
        if package_exists.is_none() {
            return Err(StoreError::PackageNotFound);
        }
        if let Some(existing) = find_version_in_transaction(&mut transaction, package_id, &request.version).await? {
            transaction.commit().await.map_err(database_error)?;
            return if existing.checksum_sha256.eq_ignore_ascii_case(&checksum) {
                Ok(PublishOutcome::AlreadyExists(existing))
            } else {
                Err(StoreError::VersionImmutable)
            };
        }
        let inserted = sqlx::query_as::<_, PackageVersionRow>(
            "INSERT INTO pckg_package_versions \
             (id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc) \
             VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7, NULL) \
             ON CONFLICT (package_id, version) DO NOTHING \
             RETURNING id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc",
        )
        .bind(id)
        .bind(package_id)
        .bind(&request.version)
        .bind(&checksum)
        .bind(&request.storage_key)
        .bind(i64::try_from(request.size_bytes).map_err(|_| StoreError::InvalidIdentifier)?)
        .bind(timestamp)
        .fetch_optional(&mut *transaction)
        .await;
        let inserted = match inserted {
            Ok(Some(row)) => row.into_domain(),
            Ok(None) => {
                // A concurrent publisher won the race. `DO NOTHING` keeps this
                // transaction usable so its definitive row can be read and the
                // checksum rule applied instead of exposing a timing-dependent
                // unique-constraint error.
                let existing = find_version_in_transaction(&mut transaction, package_id, &request.version)
                    .await?
                    .ok_or_else(|| StoreError::Database("version conflict row disappeared".into()))?;
                transaction.commit().await.map_err(database_error)?;
                return if existing.checksum_sha256.eq_ignore_ascii_case(&checksum) {
                    Ok(PublishOutcome::AlreadyExists(existing))
                } else {
                    Err(StoreError::VersionImmutable)
                };
            }
            Err(error) => return Err(database_error(error)),
        };
        transaction.commit().await.map_err(database_error)?;
        Ok(PublishOutcome::Created(inserted))
    }

    async fn find_version(&self, package_id: &str, version: &str) -> Result<Option<PackageVersion>, StoreError> {
        let package_id = parse_identifier(package_id)?;
        let row = sqlx::query_as::<_, PackageVersionRow>(
            "SELECT id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc \
             FROM pckg_package_versions WHERE package_id = $1 AND version = $2",
        )
        .bind(package_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .map_err(database_error)?;
        Ok(row.map(PackageVersionRow::into_domain))
    }

    async fn set_yanked(
        &self,
        package_id: &str,
        version: &str,
        yanked: bool,
        now_unix_seconds: i64,
    ) -> Result<PackageVersion, StoreError> {
        let package_id = parse_identifier(package_id)?;
        let timestamp = timestamp(now_unix_seconds)?;
        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        let current = find_version_in_transaction(&mut transaction, package_id, version)
            .await?
            .ok_or(StoreError::VersionNotFound)?;
        if current.is_yanked == yanked {
            return Err(if yanked { StoreError::VersionAlreadyYanked } else { StoreError::VersionNotYanked });
        }
        let row = sqlx::query_as::<_, PackageVersionRow>(
            "UPDATE pckg_package_versions SET is_yanked = $3, yanked_at_utc = CASE WHEN $3 THEN $4 ELSE NULL END \
             WHERE package_id = $1 AND version = $2 \
             RETURNING id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc",
        )
        .bind(package_id)
        .bind(version)
        .bind(yanked)
        .bind(timestamp)
        .fetch_one(&mut *transaction)
        .await
        .map_err(database_error)?;
        transaction.commit().await.map_err(database_error)?;
        Ok(row.into_domain())
    }
}

/// Community persistence failures deliberately distinguish authorization-like
/// ownership violations from missing resources so HTTP adapters can preserve
/// the legacy registry's non-disclosure policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommunityStoreError {
    InvalidAuthHubSubject,
    InvalidBoardId,
    InvalidContent,
    InvalidPackageId,
    ProfileNotFound,
    BoardNotFound,
    PostNotFound,
    CommentNotFound,
    NotificationNotFound,
    SelfVote,
    ParentCommentOutsidePost,
    Database(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityProfile {
    pub subject: String,
    pub display_name: String,
    pub bio: String,
    pub social_links_json: String,
    pub is_publisher_verified: bool,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityBoard {
    pub id: String,
    pub title: String,
    pub locked: bool,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityPost {
    pub id: i64,
    pub board_id: String,
    pub author_subject: String,
    pub title: String,
    pub content: String,
    pub score: i32,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityComment {
    pub id: i64,
    pub post_id: i64,
    pub author_subject: String,
    pub content: String,
    pub parent_comment_id: Option<i64>,
    pub score: i32,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityVote {
    Up,
    Down,
    Clear,
}

impl CommunityVote {
    fn value(self) -> i16 {
        match self {
            Self::Up => 1,
            Self::Down => -1,
            Self::Clear => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityNotificationPreference {
    pub system_enabled: bool,
    pub mention_enabled: bool,
    pub reply_enabled: bool,
    pub followed_publisher_post_enabled: bool,
    pub moderation_enabled: bool,
}

impl Default for CommunityNotificationPreference {
    fn default() -> Self {
        Self {
            system_enabled: true,
            mention_enabled: true,
            reply_enabled: true,
            followed_publisher_post_enabled: true,
            moderation_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityNotification {
    pub id: i64,
    pub recipient_subject: String,
    pub scope: String,
    pub actor_subject: String,
    pub post_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub created_at_unix_seconds: i64,
    pub read_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCommunityReview {
    pub id: String,
    pub package_id: String,
    pub author_subject: String,
    pub rating: i16,
    pub comment: String,
    pub created_at_unix_seconds: i64,
    pub updated_at_unix_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageCommunityReviewError {
    InvalidAuthHubSubject,
    InvalidPackageId,
    InvalidRating,
    InvalidComment,
    Database(String),
}

#[async_trait]
pub trait AsyncPackageCommunityReviewRepository: Send + Sync {
    async fn upsert_package_community_review(
        &self,
        review: PackageCommunityReview,
    ) -> Result<PackageCommunityReview, PackageCommunityReviewError>;
    async fn list_package_community_reviews(
        &self,
        package_id: &str,
    ) -> Result<Vec<PackageCommunityReview>, PackageCommunityReviewError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommunityNotification {
    pub recipient_subject: String,
    pub scope: String,
    pub actor_subject: String,
    pub post_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub now_unix_seconds: i64,
}

/// Async boundary consumed by a future Axum state adapter. Actor subjects are
/// explicit parameters: a repository never accepts a legacy Identity id or
/// derives identity from display data.
#[async_trait]
pub trait AsyncCommunityRepository: Send + Sync {
    async fn upsert_profile(&self, profile: CommunityProfile) -> Result<CommunityProfile, CommunityStoreError>;
    async fn profile(&self, subject: &str) -> Result<Option<CommunityProfile>, CommunityStoreError>;
    async fn boards(&self) -> Result<Vec<CommunityBoard>, CommunityStoreError>;
    async fn board(&self, board_id: &str) -> Result<Option<CommunityBoard>, CommunityStoreError>;
    async fn posts_for_board(&self, board_id: &str) -> Result<Vec<CommunityPost>, CommunityStoreError>;
    async fn post(&self, post_id: i64) -> Result<Option<CommunityPost>, CommunityStoreError>;
    async fn comments_for_post(&self, post_id: i64) -> Result<Vec<CommunityComment>, CommunityStoreError>;
    async fn create_board(&self, board: CommunityBoard) -> Result<CommunityBoard, CommunityStoreError>;
    async fn create_post(
        &self,
        board_id: &str,
        author_subject: &str,
        title: &str,
        content: &str,
        now_unix_seconds: i64,
    ) -> Result<CommunityPost, CommunityStoreError>;
    async fn create_comment(
        &self,
        post_id: i64,
        author_subject: &str,
        content: &str,
        parent_comment_id: Option<i64>,
        now_unix_seconds: i64,
    ) -> Result<CommunityComment, CommunityStoreError>;
    async fn vote_on_post(
        &self,
        post_id: i64,
        voter_subject: &str,
        vote: CommunityVote,
        now_unix_seconds: i64,
    ) -> Result<i32, CommunityStoreError>;
    async fn vote_on_comment(
        &self,
        comment_id: i64,
        voter_subject: &str,
        vote: CommunityVote,
        now_unix_seconds: i64,
    ) -> Result<i32, CommunityStoreError>;
    async fn toggle_publisher_follow(
        &self,
        follower_subject: &str,
        publisher_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<bool, CommunityStoreError>;
    async fn toggle_package_follow(
        &self,
        follower_subject: &str,
        package_id: &str,
        now_unix_seconds: i64,
    ) -> Result<bool, CommunityStoreError>;
    async fn is_following_publisher(
        &self,
        follower_subject: &str,
        publisher_subject: &str,
    ) -> Result<bool, CommunityStoreError>;
    async fn publisher_follow_count(&self, publisher_subject: &str) -> Result<i64, CommunityStoreError>;
    async fn is_following_package(&self, follower_subject: &str, package_id: &str)
    -> Result<bool, CommunityStoreError>;
    async fn package_follow_count(&self, package_id: &str) -> Result<i64, CommunityStoreError>;
    async fn set_notification_preference(
        &self,
        subject: &str,
        preference: CommunityNotificationPreference,
        now_unix_seconds: i64,
    ) -> Result<(), CommunityStoreError>;
    async fn notification_preference(
        &self,
        subject: &str,
    ) -> Result<CommunityNotificationPreference, CommunityStoreError>;
    async fn create_notification(
        &self,
        notification: NewCommunityNotification,
    ) -> Result<CommunityNotification, CommunityStoreError>;
    async fn list_notifications(&self, subject: &str) -> Result<Vec<CommunityNotification>, CommunityStoreError>;
    async fn mark_notification_read(
        &self,
        notification_id: i64,
        recipient_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<(), CommunityStoreError>;
    async fn mark_all_notifications_read(
        &self,
        recipient_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<u64, CommunityStoreError>;
    async fn create_test_notification(
        &self,
        recipient_subject: &str,
        now_unix_seconds: i64,
    ) -> Result<CommunityNotification, CommunityStoreError>;
}

/// PostgreSQL community repository. The mutation methods use transactions and
/// row locks for parent ownership, self-vote and score invariants.
#[derive(Clone, Debug)]
pub struct SqlxCommunityRepository {
    pool: PgPool,
}

impl SqlxCommunityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    pub async fn migrate(&self) -> Result<(), CommunityStoreError> {
        sqlx::raw_sql(migrations::CREATE_COMMUNITY).execute(&self.pool).await.map_err(community_database_error)?;
        sqlx::raw_sql(migrations::EXTEND_COMMUNITY_NOTIFICATIONS)
            .execute(&self.pool)
            .await
            .map_err(community_database_error)?;
        Ok(())
    }
}

#[async_trait]
impl AsyncCommunityRepository for SqlxCommunityRepository {
    async fn upsert_profile(&self, profile: CommunityProfile) -> Result<CommunityProfile, CommunityStoreError> {
        validate_community_subject(&profile.subject)?;
        validate_nonblank(&profile.display_name)?;
        let at = community_timestamp(profile.updated_at_unix_seconds)?;
        sqlx::query("INSERT INTO pckg_community_profiles (subject, display_name, bio, social_links, is_publisher_verified, updated_at_utc) VALUES ($1,$2,$3,$4::jsonb,$5,$6) ON CONFLICT (subject) DO UPDATE SET display_name=EXCLUDED.display_name,bio=EXCLUDED.bio,social_links=EXCLUDED.social_links,updated_at_utc=EXCLUDED.updated_at_utc")
            .bind(&profile.subject).bind(&profile.display_name).bind(&profile.bio).bind(&profile.social_links_json).bind(profile.is_publisher_verified).bind(at).execute(&self.pool).await.map_err(community_database_error)?;
        Ok(profile)
    }

    async fn profile(&self, subject: &str) -> Result<Option<CommunityProfile>, CommunityStoreError> {
        validate_community_subject(subject)?;
        let row = sqlx::query_as::<_, CommunityProfileRow>("SELECT subject,display_name,bio,social_links::text AS social_links_json,is_publisher_verified,updated_at_utc FROM pckg_community_profiles WHERE subject=$1").bind(subject).fetch_optional(&self.pool).await.map_err(community_database_error)?;
        Ok(row.map(CommunityProfileRow::into_domain))
    }
    async fn boards(&self) -> Result<Vec<CommunityBoard>, CommunityStoreError> {
        let rows = sqlx::query_as::<_, CommunityBoardRow>(
            "SELECT id,title,locked,created_at_utc,updated_at_utc FROM pckg_community_boards ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityBoardRow::into_domain).collect())
    }
    async fn board(&self, board_id: &str) -> Result<Option<CommunityBoard>, CommunityStoreError> {
        validate_nonblank(board_id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        let row = sqlx::query_as::<_, CommunityBoardRow>(
            "SELECT id,title,locked,created_at_utc,updated_at_utc FROM pckg_community_boards WHERE id=$1",
        )
        .bind(board_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(community_database_error)?;
        Ok(row.map(CommunityBoardRow::into_domain))
    }
    async fn posts_for_board(&self, board_id: &str) -> Result<Vec<CommunityPost>, CommunityStoreError> {
        validate_nonblank(board_id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        let rows=sqlx::query_as::<_,CommunityPostRow>("SELECT id,board_id,author_subject,title,content,score,created_at_utc,updated_at_utc FROM pckg_community_posts WHERE board_id=$1 ORDER BY created_at_utc DESC").bind(board_id).fetch_all(&self.pool).await.map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityPostRow::into_domain).collect())
    }
    async fn post(&self, post_id: i64) -> Result<Option<CommunityPost>, CommunityStoreError> {
        let row=sqlx::query_as::<_,CommunityPostRow>("SELECT id,board_id,author_subject,title,content,score,created_at_utc,updated_at_utc FROM pckg_community_posts WHERE id=$1").bind(post_id).fetch_optional(&self.pool).await.map_err(community_database_error)?;
        Ok(row.map(CommunityPostRow::into_domain))
    }
    async fn comments_for_post(&self, post_id: i64) -> Result<Vec<CommunityComment>, CommunityStoreError> {
        let rows=sqlx::query_as::<_,CommunityCommentRow>("SELECT id,post_id,author_subject,content,parent_comment_id,score,created_at_utc,updated_at_utc FROM pckg_community_comments WHERE post_id=$1 ORDER BY created_at_utc ASC").bind(post_id).fetch_all(&self.pool).await.map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityCommentRow::into_domain).collect())
    }

    async fn create_board(&self, board: CommunityBoard) -> Result<CommunityBoard, CommunityStoreError> {
        validate_nonblank(&board.id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        validate_nonblank(&board.title)?;
        let created = community_timestamp(board.created_at_unix_seconds)?;
        let updated = community_timestamp(board.updated_at_unix_seconds)?;
        sqlx::query("INSERT INTO pckg_community_boards (id,title,locked,created_at_utc,updated_at_utc) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (id) DO UPDATE SET title=EXCLUDED.title,locked=EXCLUDED.locked,updated_at_utc=EXCLUDED.updated_at_utc")
            .bind(&board.id).bind(&board.title).bind(board.locked).bind(created).bind(updated).execute(&self.pool).await.map_err(community_database_error)?;
        Ok(board)
    }

    async fn create_post(
        &self,
        board_id: &str,
        author_subject: &str,
        title: &str,
        content: &str,
        now: i64,
    ) -> Result<CommunityPost, CommunityStoreError> {
        validate_nonblank(board_id).map_err(|_| CommunityStoreError::InvalidBoardId)?;
        validate_community_subject(author_subject)?;
        validate_nonblank(title)?;
        let at = community_timestamp(now)?;
        let row = sqlx::query_as::<_, CommunityPostRow>("INSERT INTO pckg_community_posts (board_id,author_subject,title,content,score,created_at_utc,updated_at_utc) SELECT id,$2,$3,$4,0,$5,$5 FROM pckg_community_boards WHERE id=$1 AND locked=FALSE RETURNING id,board_id,author_subject,title,content,score,created_at_utc,updated_at_utc")
            .bind(board_id).bind(author_subject).bind(title).bind(content).bind(at).fetch_optional(&self.pool).await.map_err(community_database_error)?;
        row.map(CommunityPostRow::into_domain).ok_or(CommunityStoreError::BoardNotFound)
    }

    async fn create_comment(
        &self,
        post_id: i64,
        author_subject: &str,
        content: &str,
        parent_comment_id: Option<i64>,
        now: i64,
    ) -> Result<CommunityComment, CommunityStoreError> {
        validate_community_subject(author_subject)?;
        validate_nonblank(content)?;
        let at = community_timestamp(now)?;
        let mut tx = self.pool.begin().await.map_err(community_database_error)?;
        let post_exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM pckg_community_posts WHERE id=$1 FOR KEY SHARE")
                .bind(post_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(community_database_error)?;
        if post_exists.is_none() {
            return Err(CommunityStoreError::PostNotFound);
        }
        if let Some(parent) = parent_comment_id {
            let parent_post: Option<i64> =
                sqlx::query_scalar("SELECT post_id FROM pckg_community_comments WHERE id=$1 FOR KEY SHARE")
                    .bind(parent)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(community_database_error)?;
            match parent_post {
                None => return Err(CommunityStoreError::CommentNotFound),
                Some(found) if found != post_id => {
                    return Err(CommunityStoreError::ParentCommentOutsidePost);
                }
                _ => {}
            }
        }
        let row=sqlx::query_as::<_,CommunityCommentRow>("INSERT INTO pckg_community_comments (post_id,author_subject,content,parent_comment_id,score,created_at_utc,updated_at_utc) VALUES ($1,$2,$3,$4,0,$5,$5) RETURNING id,post_id,author_subject,content,parent_comment_id,score,created_at_utc,updated_at_utc").bind(post_id).bind(author_subject).bind(content).bind(parent_comment_id).bind(at).fetch_one(&mut *tx).await.map_err(community_database_error)?;
        tx.commit().await.map_err(community_database_error)?;
        Ok(row.into_domain())
    }

    async fn vote_on_post(
        &self,
        post_id: i64,
        voter: &str,
        vote: CommunityVote,
        now: i64,
    ) -> Result<i32, CommunityStoreError> {
        vote_for(&self.pool, VoteTarget::Post, post_id, voter, vote, now).await
    }
    async fn vote_on_comment(
        &self,
        comment_id: i64,
        voter: &str,
        vote: CommunityVote,
        now: i64,
    ) -> Result<i32, CommunityStoreError> {
        vote_for(&self.pool, VoteTarget::Comment, comment_id, voter, vote, now).await
    }

    async fn toggle_publisher_follow(
        &self,
        follower: &str,
        publisher: &str,
        now: i64,
    ) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        validate_community_subject(publisher)?;
        if follower == publisher {
            return Ok(true);
        }
        let at = community_timestamp(now)?;
        let removed = sqlx::query(
            "DELETE FROM pckg_community_publisher_follows WHERE follower_subject=$1 AND publisher_subject=$2",
        )
        .bind(follower)
        .bind(publisher)
        .execute(&self.pool)
        .await
        .map_err(community_database_error)?;
        if removed.rows_affected() > 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO pckg_community_publisher_follows (follower_subject,publisher_subject,created_at_utc) VALUES ($1,$2,$3)").bind(follower).bind(publisher).bind(at).execute(&self.pool).await.map_err(community_database_error)?;
        Ok(true)
    }

    async fn toggle_package_follow(
        &self,
        follower: &str,
        package_id: &str,
        now: i64,
    ) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        let package = Uuid::parse_str(package_id).map_err(|_| CommunityStoreError::InvalidPackageId)?;
        let at = community_timestamp(now)?;
        let removed =
            sqlx::query("DELETE FROM pckg_community_package_follows WHERE follower_subject=$1 AND package_id=$2")
                .bind(follower)
                .bind(package)
                .execute(&self.pool)
                .await
                .map_err(community_database_error)?;
        if removed.rows_affected() > 0 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO pckg_community_package_follows (follower_subject,package_id,created_at_utc) VALUES ($1,$2,$3)",
        )
        .bind(follower)
        .bind(package)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(community_database_error)?;
        Ok(true)
    }

    async fn is_following_publisher(&self, follower: &str, publisher: &str) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        validate_community_subject(publisher)?;
        if follower == publisher {
            return Ok(true);
        }
        let value:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pckg_community_publisher_follows WHERE follower_subject=$1 AND publisher_subject=$2)").bind(follower).bind(publisher).fetch_one(&self.pool).await.map_err(community_database_error)?;
        Ok(value)
    }
    async fn publisher_follow_count(&self, publisher: &str) -> Result<i64, CommunityStoreError> {
        validate_community_subject(publisher)?;
        sqlx::query_scalar("SELECT COUNT(*) FROM pckg_community_publisher_follows WHERE publisher_subject=$1")
            .bind(publisher)
            .fetch_one(&self.pool)
            .await
            .map_err(community_database_error)
    }
    async fn is_following_package(&self, follower: &str, package_id: &str) -> Result<bool, CommunityStoreError> {
        validate_community_subject(follower)?;
        let package = Uuid::parse_str(package_id).map_err(|_| CommunityStoreError::InvalidPackageId)?;
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pckg_community_package_follows WHERE follower_subject=$1 AND package_id=$2)",
        )
        .bind(follower)
        .bind(package)
        .fetch_one(&self.pool)
        .await
        .map_err(community_database_error)
    }
    async fn package_follow_count(&self, package_id: &str) -> Result<i64, CommunityStoreError> {
        let package = Uuid::parse_str(package_id).map_err(|_| CommunityStoreError::InvalidPackageId)?;
        sqlx::query_scalar("SELECT COUNT(*) FROM pckg_community_package_follows WHERE package_id=$1")
            .bind(package)
            .fetch_one(&self.pool)
            .await
            .map_err(community_database_error)
    }

    async fn set_notification_preference(
        &self,
        subject: &str,
        preference: CommunityNotificationPreference,
        now: i64,
    ) -> Result<(), CommunityStoreError> {
        validate_community_subject(subject)?;
        let at = community_timestamp(now)?;
        sqlx::query("INSERT INTO pckg_community_notification_preferences (subject,system_enabled,mention_enabled,reply_enabled,followed_publisher_post_enabled,moderation_enabled,updated_at_utc) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (subject) DO UPDATE SET system_enabled=EXCLUDED.system_enabled,mention_enabled=EXCLUDED.mention_enabled,reply_enabled=EXCLUDED.reply_enabled,followed_publisher_post_enabled=EXCLUDED.followed_publisher_post_enabled,moderation_enabled=EXCLUDED.moderation_enabled,updated_at_utc=EXCLUDED.updated_at_utc").bind(subject).bind(preference.system_enabled).bind(preference.mention_enabled).bind(preference.reply_enabled).bind(preference.followed_publisher_post_enabled).bind(preference.moderation_enabled).bind(at).execute(&self.pool).await.map_err(community_database_error)?;
        Ok(())
    }

    async fn notification_preference(
        &self,
        subject: &str,
    ) -> Result<CommunityNotificationPreference, CommunityStoreError> {
        validate_community_subject(subject)?;
        let row:Option<CommunityPreferenceRow>=sqlx::query_as("SELECT system_enabled,mention_enabled,reply_enabled,followed_publisher_post_enabled,moderation_enabled FROM pckg_community_notification_preferences WHERE subject=$1").bind(subject).fetch_optional(&self.pool).await.map_err(community_database_error)?;
        Ok(row.map(CommunityPreferenceRow::into_domain).unwrap_or_default())
    }

    async fn create_notification(
        &self,
        notification: NewCommunityNotification,
    ) -> Result<CommunityNotification, CommunityStoreError> {
        validate_community_subject(&notification.recipient_subject)?;
        validate_community_subject(&notification.actor_subject)?;
        validate_notification_scope(&notification.scope)?;
        let at = community_timestamp(notification.now_unix_seconds)?;
        let row = sqlx::query_as::<_, CommunityNotificationRow>(
            "INSERT INTO pckg_community_notifications (recipient_subject,scope,actor_subject,post_id,comment_id,created_at_utc,read_at_utc) \
             VALUES ($1,$2,$3,$4,$5,$6,NULL) \
             RETURNING id,recipient_subject,scope,actor_subject,post_id,comment_id,created_at_utc,read_at_utc",
        )
        .bind(&notification.recipient_subject)
        .bind(&notification.scope)
        .bind(&notification.actor_subject)
        .bind(notification.post_id)
        .bind(notification.comment_id)
        .bind(at)
        .fetch_one(&self.pool)
        .await
        .map_err(community_database_error)?;
        Ok(row.into_domain())
    }

    async fn list_notifications(&self, subject: &str) -> Result<Vec<CommunityNotification>, CommunityStoreError> {
        validate_community_subject(subject)?;
        let rows=sqlx::query_as::<_,CommunityNotificationRow>("SELECT id,recipient_subject,scope,actor_subject,post_id,comment_id,created_at_utc,read_at_utc FROM pckg_community_notifications WHERE recipient_subject=$1 ORDER BY created_at_utc DESC").bind(subject).fetch_all(&self.pool).await.map_err(community_database_error)?;
        Ok(rows.into_iter().map(CommunityNotificationRow::into_domain).collect())
    }

    async fn mark_notification_read(
        &self,
        notification_id: i64,
        recipient: &str,
        now: i64,
    ) -> Result<(), CommunityStoreError> {
        validate_community_subject(recipient)?;
        let at = community_timestamp(now)?;
        let result=sqlx::query("UPDATE pckg_community_notifications SET read_at_utc=COALESCE(read_at_utc,$3) WHERE id=$1 AND recipient_subject=$2").bind(notification_id).bind(recipient).bind(at).execute(&self.pool).await.map_err(community_database_error)?;
        if result.rows_affected() == 0 { Err(CommunityStoreError::NotificationNotFound) } else { Ok(()) }
    }

    async fn mark_all_notifications_read(&self, recipient: &str, now: i64) -> Result<u64, CommunityStoreError> {
        validate_community_subject(recipient)?;
        let at = community_timestamp(now)?;
        sqlx::query(
            "UPDATE pckg_community_notifications SET read_at_utc=$2 WHERE recipient_subject=$1 AND read_at_utc IS NULL",
        )
        .bind(recipient)
        .bind(at)
        .execute(&self.pool)
        .await
        .map(|result| result.rows_affected())
        .map_err(community_database_error)
    }

    async fn create_test_notification(
        &self,
        recipient: &str,
        now: i64,
    ) -> Result<CommunityNotification, CommunityStoreError> {
        validate_community_subject(recipient)?;
        let at = community_timestamp(now)?;
        // A delivery check must work before an account edits its profile. The
        // fallback stores only the stable subject, never a login or email.
        sqlx::query(CREATE_TEST_NOTIFICATION_PROFILE_SQL)
            .bind(recipient)
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(community_database_error)?;
        self.create_notification(NewCommunityNotification {
            recipient_subject: recipient.to_owned(),
            scope: "system".to_owned(),
            actor_subject: recipient.to_owned(),
            post_id: None,
            comment_id: None,
            now_unix_seconds: now,
        })
        .await
    }
}

#[derive(sqlx::FromRow)]
struct CommunityPostRow {
    id: i64,
    board_id: String,
    author_subject: String,
    title: String,
    content: String,
    score: i32,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct CommunityProfileRow {
    subject: String,
    display_name: String,
    bio: String,
    social_links_json: String,
    is_publisher_verified: bool,
    updated_at_utc: DateTime<Utc>,
}
impl CommunityProfileRow {
    fn into_domain(self) -> CommunityProfile {
        CommunityProfile {
            subject: self.subject,
            display_name: self.display_name,
            bio: self.bio,
            social_links_json: self.social_links_json,
            is_publisher_verified: self.is_publisher_verified,
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CommunityBoardRow {
    id: String,
    title: String,
    locked: bool,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}
impl CommunityBoardRow {
    fn into_domain(self) -> CommunityBoard {
        CommunityBoard {
            id: self.id,
            title: self.title,
            locked: self.locked,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CommunityPreferenceRow {
    system_enabled: bool,
    mention_enabled: bool,
    reply_enabled: bool,
    followed_publisher_post_enabled: bool,
    moderation_enabled: bool,
}
impl CommunityPreferenceRow {
    fn into_domain(self) -> CommunityNotificationPreference {
        CommunityNotificationPreference {
            system_enabled: self.system_enabled,
            mention_enabled: self.mention_enabled,
            reply_enabled: self.reply_enabled,
            followed_publisher_post_enabled: self.followed_publisher_post_enabled,
            moderation_enabled: self.moderation_enabled,
        }
    }
}
impl CommunityPostRow {
    fn into_domain(self) -> CommunityPost {
        CommunityPost {
            id: self.id,
            board_id: self.board_id,
            author_subject: self.author_subject,
            title: self.title,
            content: self.content,
            score: self.score,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CommunityCommentRow {
    id: i64,
    post_id: i64,
    author_subject: String,
    content: String,
    parent_comment_id: Option<i64>,
    score: i32,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}
impl CommunityCommentRow {
    fn into_domain(self) -> CommunityComment {
        CommunityComment {
            id: self.id,
            post_id: self.post_id,
            author_subject: self.author_subject,
            content: self.content,
            parent_comment_id: self.parent_comment_id,
            score: self.score,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct CommunityNotificationRow {
    id: i64,
    recipient_subject: String,
    scope: String,
    actor_subject: String,
    post_id: Option<i64>,
    comment_id: Option<i64>,
    created_at_utc: DateTime<Utc>,
    read_at_utc: Option<DateTime<Utc>>,
}
impl CommunityNotificationRow {
    fn into_domain(self) -> CommunityNotification {
        CommunityNotification {
            id: self.id,
            recipient_subject: self.recipient_subject,
            scope: self.scope,
            actor_subject: self.actor_subject,
            post_id: self.post_id,
            comment_id: self.comment_id,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            read_at_unix_seconds: self.read_at_utc.map(|v| v.timestamp()),
        }
    }
}

#[derive(Clone, Copy)]
enum VoteTarget {
    Post,
    Comment,
}

impl VoteTarget {
    const fn content_table(self) -> &'static str {
        match self {
            Self::Post => "pckg_community_posts",
            Self::Comment => "pckg_community_comments",
        }
    }

    const fn vote_table(self) -> &'static str {
        match self {
            Self::Post => "pckg_community_post_votes",
            Self::Comment => "pckg_community_comment_votes",
        }
    }

    const fn key_column(self) -> &'static str {
        match self {
            Self::Post => "post_id",
            Self::Comment => "comment_id",
        }
    }

    const fn missing_error(self) -> CommunityStoreError {
        match self {
            Self::Post => CommunityStoreError::PostNotFound,
            Self::Comment => CommunityStoreError::CommentNotFound,
        }
    }
}

async fn vote_for(
    pool: &PgPool,
    target: VoteTarget,
    content_id: i64,
    voter: &str,
    vote: CommunityVote,
    now: i64,
) -> Result<i32, CommunityStoreError> {
    let content_table = target.content_table();
    let vote_table = target.vote_table();
    let key_column = target.key_column();
    validate_community_subject(voter)?;
    let at = community_timestamp(now)?;
    let mut tx = pool.begin().await.map_err(community_database_error)?;
    // Identifiers are private constants supplied by this module, never HTTP input.
    let author_sql = format!("SELECT author_subject FROM {content_table} WHERE id=$1 FOR UPDATE");
    let author: Option<String> = sqlx::query_scalar(&author_sql)
        .bind(content_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(community_database_error)?;
    let author = author.ok_or(target.missing_error())?;
    if author == voter {
        return Err(CommunityStoreError::SelfVote);
    }
    let existing_sql = format!("SELECT value FROM {vote_table} WHERE {key_column}=$1 AND voter_subject=$2 FOR UPDATE");
    let old: Option<i16> = sqlx::query_scalar(&existing_sql)
        .bind(content_id)
        .bind(voter)
        .fetch_optional(&mut *tx)
        .await
        .map_err(community_database_error)?;
    let new = vote.value();
    if new == 0 {
        let delete_sql = format!("DELETE FROM {vote_table} WHERE {key_column}=$1 AND voter_subject=$2");
        sqlx::query(&delete_sql)
            .bind(content_id)
            .bind(voter)
            .execute(&mut *tx)
            .await
            .map_err(community_database_error)?;
    } else {
        let upsert_sql = format!(
            "INSERT INTO {vote_table} ({key_column},voter_subject,value,updated_at_utc) VALUES ($1,$2,$3,$4) ON CONFLICT ({key_column},voter_subject) DO UPDATE SET value=EXCLUDED.value,updated_at_utc=EXCLUDED.updated_at_utc"
        );
        sqlx::query(&upsert_sql)
            .bind(content_id)
            .bind(voter)
            .bind(new)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(community_database_error)?;
    }
    let delta = i32::from(new) - i32::from(old.unwrap_or(0));
    let update_sql = format!("UPDATE {content_table} SET score=score+$2,updated_at_utc=$3 WHERE id=$1 RETURNING score");
    let score: i32 = sqlx::query_scalar(&update_sql)
        .bind(content_id)
        .bind(delta)
        .bind(at)
        .fetch_one(&mut *tx)
        .await
        .map_err(community_database_error)?;
    tx.commit().await.map_err(community_database_error)?;
    Ok(score)
}

fn validate_community_subject(subject: &str) -> Result<(), CommunityStoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(CommunityStoreError::InvalidAuthHubSubject)
}
fn validate_nonblank(value: &str) -> Result<(), CommunityStoreError> {
    (!value.trim().is_empty() && value == value.trim()).then_some(()).ok_or(CommunityStoreError::InvalidContent)
}
fn validate_notification_scope(scope: &str) -> Result<(), CommunityStoreError> {
    matches!(scope, "system" | "mention" | "reply" | "followed_publisher_post" | "moderation")
        .then_some(())
        .ok_or(CommunityStoreError::InvalidContent)
}
fn community_timestamp(value: i64) -> Result<DateTime<Utc>, CommunityStoreError> {
    DateTime::from_timestamp(value, 0).ok_or(CommunityStoreError::InvalidContent)
}
fn community_database_error(error: sqlx::Error) -> CommunityStoreError {
    CommunityStoreError::Database(error.to_string())
}

#[derive(sqlx::FromRow)]
struct PackageRow {
    id: Uuid,
    name: String,
    owner_subject: String,
    is_public: bool,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct UnmappedLegacyIdentityRow {
    legacy_identity_id: String,
    package_count: i64,
}

impl UnmappedLegacyIdentityRow {
    fn into_domain(self) -> Result<UnmappedLegacyIdentity, StoreError> {
        Ok(UnmappedLegacyIdentity {
            legacy_identity_id: self.legacy_identity_id,
            package_count: as_u64(self.package_count)?,
        })
    }
}

impl PackageRow {
    fn into_domain(self) -> Package {
        Package {
            id: self.id.to_string(),
            name: self.name,
            owner_subject: self.owner_subject,
            is_public: self.is_public,
            created_at_unix_seconds: self.created_at_utc.timestamp(),
            updated_at_unix_seconds: self.updated_at_utc.timestamp(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct PackageVersionRow {
    id: Uuid,
    package_id: Uuid,
    version: String,
    checksum_sha256: String,
    storage_key: String,
    size_bytes: i64,
    is_yanked: bool,
    published_at_utc: DateTime<Utc>,
    yanked_at_utc: Option<DateTime<Utc>>,
}

impl PackageVersionRow {
    fn into_domain(self) -> PackageVersion {
        PackageVersion {
            id: self.id.to_string(),
            package_id: self.package_id.to_string(),
            version: self.version,
            checksum_sha256: self.checksum_sha256,
            storage_key: self.storage_key,
            size_bytes: self.size_bytes as u64,
            is_yanked: self.is_yanked,
            published_at_unix_seconds: self.published_at_utc.timestamp(),
            yanked_at_unix_seconds: self.yanked_at_utc.map(|value| value.timestamp()),
        }
    }
}

async fn find_version_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    package_id: Uuid,
    version: &str,
) -> Result<Option<PackageVersion>, StoreError> {
    sqlx::query_as::<_, PackageVersionRow>(
        "SELECT id, package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, published_at_utc, yanked_at_utc \
         FROM pckg_package_versions WHERE package_id = $1 AND version = $2 FOR UPDATE",
    )
    .bind(package_id)
    .bind(version)
    .fetch_optional(&mut **transaction)
    .await
    .map(|row| row.map(PackageVersionRow::into_domain))
    .map_err(database_error)
}

fn parse_identifier(value: &str) -> Result<Uuid, StoreError> {
    Uuid::parse_str(value).map_err(|_| StoreError::InvalidIdentifier)
}

fn as_u64(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::InvalidIdentifier)
}

fn timestamp(value: i64) -> Result<DateTime<Utc>, StoreError> {
    DateTime::from_timestamp(value, 0).ok_or(StoreError::InvalidIdentifier)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error.as_database_error().and_then(|database| database.code()).is_some_and(|code| code == "23505")
}

fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Database(error.to_string())
}

fn validate_cutover_request(request: &LegacyIdentityCutoverRequest) -> Result<(), LegacyIdentityCutoverError> {
    parse_identifier(&request.run_id)?;
    if request.requested_by.trim().is_empty() || request.requested_by != request.requested_by.trim() {
        return Err(LegacyIdentityCutoverError::InvalidRequest("requested_by must be non-empty and trimmed".into()));
    }
    timestamp(request.now_unix_seconds)?;
    let mut identity_subjects = BTreeMap::new();
    for mapping in &request.mappings {
        if mapping.legacy_identity_id.trim().is_empty()
            || mapping.legacy_identity_id != mapping.legacy_identity_id.trim()
        {
            return Err(LegacyIdentityCutoverError::InvalidRequest(
                "legacy_identity_id must be non-empty and trimmed".into(),
            ));
        }
        validate_subject(&mapping.github_subject)?;
        if mapping.approved_by.trim().is_empty() || mapping.approved_by != mapping.approved_by.trim() {
            return Err(LegacyIdentityCutoverError::InvalidRequest("approved_by must be non-empty and trimmed".into()));
        }
        timestamp(mapping.approved_at_unix_seconds)?;
        if identity_subjects.insert(&mapping.legacy_identity_id, &mapping.github_subject).is_some() {
            return Err(LegacyIdentityCutoverError::InvalidRequest(format!(
                "legacy identity `{}` is mapped more than once in this run",
                mapping.legacy_identity_id
            )));
        }
    }
    Ok(())
}

async fn write_cutover_report(
    transaction: &mut Transaction<'_, Postgres>,
    run_id: Uuid,
    report: &LegacyIdentityCutoverReport,
    completed_at: DateTime<Utc>,
) -> Result<(), StoreError> {
    let status = match report.status {
        LegacyIdentityCutoverStatus::RejectedUnmappedIdentity => "rejected_unmapped_identity",
        LegacyIdentityCutoverStatus::Completed => "completed",
    };
    sqlx::query(
        "UPDATE pckg_legacy_identity_cutover_runs \
         SET completed_at_utc = $2, status = $3, mapped_identity_count = $4, \
             legacy_package_count = $5, imported_package_count = $6, imported_version_count = $7 \
         WHERE run_id = $1",
    )
    .bind(run_id)
    .bind(completed_at)
    .bind(status)
    .bind(i64::try_from(report.mapped_identity_count).map_err(|_| StoreError::InvalidIdentifier)?)
    .bind(i64::try_from(report.legacy_package_count).map_err(|_| StoreError::InvalidIdentifier)?)
    .bind(i64::try_from(report.imported_package_count).map_err(|_| StoreError::InvalidIdentifier)?)
    .bind(i64::try_from(report.imported_version_count).map_err(|_| StoreError::InvalidIdentifier)?)
    .execute(&mut **transaction)
    .await
    .map_err(database_error)?;
    Ok(())
}

/// Deterministic test double. The Postgres adapter must preserve the outcomes
/// exposed by [`PackageRepository`], including checksum idempotency.
#[derive(Debug, Default, Clone)]
pub struct InMemoryPackageRepository {
    packages_by_name: BTreeMap<String, Package>,
    versions_by_key: BTreeMap<(String, String), PackageVersion>,
}

impl PackageRepository for InMemoryPackageRepository {
    fn create_package(&mut self, request: NewPackage) -> Result<Package, StoreError> {
        validate_package_name(&request.name)?;
        validate_subject(&request.owner_subject)?;
        if self.packages_by_name.contains_key(&request.name) {
            return Err(StoreError::PackageAlreadyExists);
        }
        let package = Package {
            id: request.id,
            name: request.name,
            owner_subject: request.owner_subject,
            is_public: request.is_public,
            created_at_unix_seconds: request.now_unix_seconds,
            updated_at_unix_seconds: request.now_unix_seconds,
        };
        self.packages_by_name.insert(package.name.clone(), package.clone());
        Ok(package)
    }

    fn find_package(&self, name: &str) -> Option<&Package> {
        self.packages_by_name.get(name)
    }

    fn delete_package(&mut self, name: &str) -> Result<Vec<PackageVersion>, StoreError> {
        let package = self.packages_by_name.remove(name).ok_or(StoreError::PackageNotFound)?;
        let keys = self
            .versions_by_key
            .keys()
            .filter(|(package_id, _)| package_id == &package.id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(keys.into_iter().filter_map(|key| self.versions_by_key.remove(&key)).collect())
    }

    fn publish_version(&mut self, request: PublishVersion) -> Result<PublishOutcome, StoreError> {
        validate_version(&request.version)?;
        validate_checksum(&request.checksum_sha256)?;
        if !self.packages_by_name.values().any(|package| package.id == request.package_id) {
            return Err(StoreError::PackageNotFound);
        }
        let key = (request.package_id.clone(), request.version.clone());
        if let Some(existing) = self.versions_by_key.get(&key) {
            return if existing.checksum_sha256.eq_ignore_ascii_case(&request.checksum_sha256) {
                Ok(PublishOutcome::AlreadyExists(existing.clone()))
            } else {
                Err(StoreError::VersionImmutable)
            };
        }
        let version = PackageVersion {
            id: request.id,
            package_id: request.package_id,
            version: request.version,
            checksum_sha256: request.checksum_sha256.to_ascii_lowercase(),
            storage_key: request.storage_key,
            size_bytes: request.size_bytes,
            is_yanked: false,
            published_at_unix_seconds: request.now_unix_seconds,
            yanked_at_unix_seconds: None,
        };
        self.versions_by_key.insert(key, version.clone());
        Ok(PublishOutcome::Created(version))
    }

    fn find_version(&self, package_id: &str, version: &str) -> Option<&PackageVersion> {
        self.versions_by_key.get(&(package_id.to_owned(), version.to_owned()))
    }

    fn set_yanked(
        &mut self,
        package_id: &str,
        version: &str,
        yanked: bool,
        now_unix_seconds: i64,
    ) -> Result<PackageVersion, StoreError> {
        let entity = self
            .versions_by_key
            .get_mut(&(package_id.to_owned(), version.to_owned()))
            .ok_or(StoreError::VersionNotFound)?;
        if entity.is_yanked == yanked {
            return Err(if yanked { StoreError::VersionAlreadyYanked } else { StoreError::VersionNotYanked });
        }
        entity.is_yanked = yanked;
        entity.yanked_at_unix_seconds = yanked.then_some(now_unix_seconds);
        Ok(entity.clone())
    }
}

fn validate_package_name(name: &str) -> Result<(), StoreError> {
    (!name.trim().is_empty() && name == name.trim()).then_some(()).ok_or(StoreError::InvalidPackageName)
}

fn validate_subject(subject: &str) -> Result<(), StoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(StoreError::InvalidAuthHubSubject)
}

fn validate_version(version: &str) -> Result<(), StoreError> {
    (!version.trim().is_empty() && version == version.trim()).then_some(()).ok_or(StoreError::InvalidVersion)
}

fn validate_checksum(checksum: &str) -> Result<(), StoreError> {
    (checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(())
        .ok_or(StoreError::InvalidChecksum)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKSUM: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn package_request() -> NewPackage {
        NewPackage {
            id: "package-1".into(),
            name: "beskid.demo".into(),
            owner_subject: "github:42".into(),
            is_public: true,
            now_unix_seconds: 100,
        }
    }
    fn publish_request() -> PublishVersion {
        PublishVersion {
            id: "version-1".into(),
            package_id: "package-1".into(),
            version: "1.0.0".into(),
            checksum_sha256: CHECKSUM.into(),
            storage_key: "beskid.demo/1.0.0.bpk".into(),
            size_bytes: 12,
            now_unix_seconds: 200,
        }
    }

    #[test]
    fn package_owner_is_a_github_auth_hub_subject() {
        let mut repository = InMemoryPackageRepository::default();
        let package = repository.create_package(package_request()).unwrap();
        assert_eq!(package.owner_subject, "github:42");
        assert_eq!(
            repository.create_package(NewPackage { owner_subject: "identity-user-id".into(), ..package_request() }),
            Err(StoreError::InvalidAuthHubSubject)
        );
    }

    #[test]
    fn package_names_are_unique() {
        let mut repository = InMemoryPackageRepository::default();
        repository.create_package(package_request()).unwrap();
        assert_eq!(repository.create_package(package_request()), Err(StoreError::PackageAlreadyExists));
    }

    #[test]
    fn publish_is_idempotent_only_for_matching_checksum() {
        let mut repository = InMemoryPackageRepository::default();
        repository.create_package(package_request()).unwrap();
        assert!(matches!(repository.publish_version(publish_request()), Ok(PublishOutcome::Created(_))));
        assert!(matches!(repository.publish_version(publish_request()), Ok(PublishOutcome::AlreadyExists(_))));
        assert_eq!(
            repository.publish_version(PublishVersion { checksum_sha256: "f".repeat(64), ..publish_request() }),
            Err(StoreError::VersionImmutable)
        );
    }

    #[test]
    fn yanking_is_reversible_but_state_transitions_are_not_idempotent() {
        let mut repository = InMemoryPackageRepository::default();
        repository.create_package(package_request()).unwrap();
        repository.publish_version(publish_request()).unwrap();
        let yanked = repository.set_yanked("package-1", "1.0.0", true, 300).unwrap();
        assert_eq!(yanked.yanked_at_unix_seconds, Some(300));
        assert_eq!(repository.set_yanked("package-1", "1.0.0", true, 301), Err(StoreError::VersionAlreadyYanked));
        let restored = repository.set_yanked("package-1", "1.0.0", false, 302).unwrap();
        assert_eq!(restored.yanked_at_unix_seconds, None);
    }

    #[test]
    fn migration_has_database_enforced_immutability_keys() {
        assert!(migrations::CREATE_PACKAGE_REGISTRY.contains("UNIQUE (name)"));
        assert!(migrations::CREATE_PACKAGE_REGISTRY.contains("UNIQUE (package_id, version)"));
        assert!(migrations::BACKFILL_REQUIRES_SUBJECT_MAPPING.contains("Do not infer subjects"));
        assert!(migrations::LEGACY_IDENTITY_CUTOVER_AUDIT.contains("pckg_legacy_identity_cutover_unmapped_identities"));
        assert!(migrations::LEGACY_IDENTITY_CUTOVER_AUDIT.contains("'^github:[0-9]+$'"));
    }

    #[test]
    fn package_review_queue_migration_retains_auth_hub_subjects_and_valid_actions() {
        assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("pckg_package_review_requests"));
        assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("'^github:[0-9]+$'"));
        assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("'pending', 'approved', 'needs_changes', 'rejected'"));
        assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("reviewer_subject"));
    }

    #[test]
    fn community_migration_keys_every_identity_to_an_auth_hub_subject() {
        assert!(migrations::CREATE_COMMUNITY.contains("pckg_community_profiles"));
        assert!(migrations::CREATE_COMMUNITY.contains("'^github:[0-9]+$'"));
        assert!(migrations::CREATE_COMMUNITY.contains("pckg_community_post_votes"));
        assert!(migrations::CREATE_COMMUNITY.contains("UNIQUE (post_id, voter_subject)"));
        assert!(migrations::CREATE_COMMUNITY.contains("pckg_community_notification_preferences"));
        assert!(migrations::CREATE_COMMUNITY.contains("recipient_subject"));
    }

    #[test]
    fn test_notification_profile_insert_matches_community_profile_schema() {
        assert!(migrations::CREATE_COMMUNITY.contains("social_links JSONB"));
        assert!(migrations::CREATE_COMMUNITY.contains("updated_at_utc TIMESTAMPTZ"));
        assert!(CREATE_TEST_NOTIFICATION_PROFILE_SQL.contains("social_links"));
        assert!(CREATE_TEST_NOTIFICATION_PROFILE_SQL.contains("updated_at_utc"));
        assert!(!CREATE_TEST_NOTIFICATION_PROFILE_SQL.contains("social_links_json"));
        assert!(!CREATE_TEST_NOTIFICATION_PROFILE_SQL.contains("created_at_utc"));
    }

    #[test]
    fn cutover_input_accepts_only_explicit_github_subject_mappings() {
        let request = LegacyIdentityCutoverRequest {
            run_id: "c48d3968-7b0f-4a70-89cd-102607f6a6b9".into(),
            requested_by: "migration-operator".into(),
            now_unix_seconds: 100,
            mappings: vec![LegacyIdentitySubjectMapping {
                legacy_identity_id: "legacy-identity-primary-key".into(),
                github_subject: "github:42".into(),
                approved_by: "security-reviewer".into(),
                approved_at_unix_seconds: 99,
            }],
        };
        assert_eq!(validate_cutover_request(&request), Ok(()));
        assert!(matches!(
            validate_cutover_request(&LegacyIdentityCutoverRequest {
                mappings: vec![LegacyIdentitySubjectMapping {
                    github_subject: "email@example.test".into(),
                    ..request.mappings[0].clone()
                }],
                ..request.clone()
            }),
            Err(LegacyIdentityCutoverError::Store(StoreError::InvalidAuthHubSubject))
        ));
    }

    #[test]
    fn cutover_input_rejects_duplicate_legacy_identity_mapping() {
        let mapping = LegacyIdentitySubjectMapping {
            legacy_identity_id: "legacy-identity-primary-key".into(),
            github_subject: "github:42".into(),
            approved_by: "security-reviewer".into(),
            approved_at_unix_seconds: 99,
        };
        let request = LegacyIdentityCutoverRequest {
            run_id: "c48d3968-7b0f-4a70-89cd-102607f6a6b9".into(),
            requested_by: "migration-operator".into(),
            now_unix_seconds: 100,
            mappings: vec![mapping.clone(), mapping],
        };
        assert!(matches!(
            validate_cutover_request(&request),
            Err(LegacyIdentityCutoverError::InvalidRequest(message))
                if message.contains("mapped more than once")
        ));
    }
}
