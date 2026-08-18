//! Ordered PostgreSQL migration plan for the package registry.
//!
//! Community/forum data is owned by NodeBB and is not persisted by the
//! registry. Legacy ASP.NET Identity cutover is retired with the .NET server;
//! no cutover audit tables are recreated. Subjects are stable Authelia
//! usernames (`Remote-User`) or carried-over `github:<numeric-id>` values.

use crate::package::{SqlxPackageRepository, StoreError, database_error};

/// Creates canonical package and immutable package-version records.
pub const CREATE_PACKAGE_REGISTRY: &str = include_str!("../migrations/0001_create_package_registry.sql");

/// API keys are pckg-owned automation credentials. The table retains only
/// a SHA-256 digest, never an issued raw token.
pub const CREATE_API_KEYS: &str = include_str!("../migrations/0005_create_api_keys.sql");

/// Publisher verification, resource grants and package-review audit decisions.
/// Roles are projected from Authelia groups, so no role table is created here;
/// the schema intentionally seeds no administrator.
pub const CREATE_ADMINISTRATION: &str = include_str!("../migrations/0006_create_administration.sql");

/// Durable review submission queue and the current reviewer disposition.
pub const CREATE_PACKAGE_REVIEW_QUEUE: &str = include_str!("../migrations/0008_create_package_review_queue.sql");

/// Registry operations: blocked-link policy, activity log and the weekly
/// in-app spotlight audit. SMTP/email surfaces are deliberately not recreated.
pub const CREATE_REGISTRY_OPERATIONS: &str = include_str!("../migrations/0009_create_registry_operations.sql");

/// Package-scoped reviews (rating + comment). Forum-style community is NodeBB.
pub const CREATE_PACKAGE_COMMUNITY_REVIEWS: &str =
    include_str!("../migrations/0010_create_package_community_reviews.sql");

pub const ALL: &[(&str, &str)] = &[
    ("0001_create_package_registry", CREATE_PACKAGE_REGISTRY),
    ("0005_create_api_keys", CREATE_API_KEYS),
    ("0006_create_administration", CREATE_ADMINISTRATION),
    ("0008_create_package_review_queue", CREATE_PACKAGE_REVIEW_QUEUE),
    ("0009_create_registry_operations", CREATE_REGISTRY_OPERATIONS),
    ("0010_create_package_community_reviews", CREATE_PACKAGE_COMMUNITY_REVIEWS),
];

impl SqlxPackageRepository {
    /// Applies the registry-owned migrations in order.
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
