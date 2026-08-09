//! Ordered PostgreSQL migration plan for the package-registry cutover.

use crate::package::{database_error, SqlxPackageRepository, StoreError};

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

impl SqlxPackageRepository {
    /// Applies the registry-owned migrations. Legacy data import intentionally
    /// remains separate because it requires a reviewed subject mapping.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        for (_, migration) in ALL {
            if migration.lines().all(|line| line.trim().is_empty() || line.trim_start().starts_with("--")) {
                continue;
            }
            sqlx::raw_sql(migration).execute(self.pool()).await.map_err(database_error)?;
        }
        Ok(())
    }
}
