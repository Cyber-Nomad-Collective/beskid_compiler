//! Package registry persistence boundary.
//!
//! The production adapter will be backed by PostgreSQL; this crate keeps its
//! domain rules executable without choosing a SQL runtime prematurely.  All
//! owners are stable Auth Hub subjects (for example, `github:12345`), never a
//! legacy ASP.NET Identity id.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub mod migrations {
    //! Ordered PostgreSQL migration plan for the package-registry cutover.

    /// Creates canonical package and immutable package-version records.
    pub const CREATE_PACKAGE_REGISTRY: &str =
        include_str!("../migrations/0001_create_package_registry.sql");

    /// Cutover is intentionally explicit: Identity ids need a reviewed mapping
    /// to Auth Hub subjects before this statement can be used in production.
    pub const BACKFILL_REQUIRES_SUBJECT_MAPPING: &str =
        include_str!("../migrations/0002_backfill_requires_subject_mapping.sql");

    pub const ALL: &[(&str, &str)] = &[
        ("0001_create_package_registry", CREATE_PACKAGE_REGISTRY),
        (
            "0002_backfill_requires_subject_mapping",
            BACKFILL_REQUIRES_SUBJECT_MAPPING,
        ),
    ];
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
    async fn publish_version(&self, request: PublishVersion) -> Result<PublishOutcome, StoreError>;
    async fn find_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<PackageVersion>, StoreError>;
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
            if migration.trim_start().starts_with("--") {
                continue;
            }
            sqlx::raw_sql(migration)
                .execute(&self.pool)
                .await
                .map_err(database_error)?;
        }
        Ok(())
    }
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

    async fn publish_version(&self, request: PublishVersion) -> Result<PublishOutcome, StoreError> {
        validate_version(&request.version)?;
        validate_checksum(&request.checksum_sha256)?;
        let id = parse_identifier(&request.id)?;
        let package_id = parse_identifier(&request.package_id)?;
        let timestamp = timestamp(request.now_unix_seconds)?;
        let checksum = request.checksum_sha256.to_ascii_lowercase();
        let mut transaction = self.pool.begin().await.map_err(database_error)?;
        let package_exists = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM pckg_packages WHERE id = $1 FOR KEY SHARE",
        )
        .bind(package_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(database_error)?;
        if package_exists.is_none() {
            return Err(StoreError::PackageNotFound);
        }
        if let Some(existing) =
            find_version_in_transaction(&mut transaction, package_id, &request.version).await?
        {
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
                let existing =
                    find_version_in_transaction(&mut transaction, package_id, &request.version)
                        .await?
                        .ok_or_else(|| {
                            StoreError::Database("version conflict row disappeared".into())
                        })?;
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

    async fn find_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<PackageVersion>, StoreError> {
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
            return Err(if yanked {
                StoreError::VersionAlreadyYanked
            } else {
                StoreError::VersionNotYanked
            });
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

#[derive(sqlx::FromRow)]
struct PackageRow {
    id: Uuid,
    name: String,
    owner_subject: String,
    is_public: bool,
    created_at_utc: DateTime<Utc>,
    updated_at_utc: DateTime<Utc>,
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

fn timestamp(value: i64) -> Result<DateTime<Utc>, StoreError> {
    DateTime::from_timestamp(value, 0).ok_or(StoreError::InvalidIdentifier)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|database| database.code())
        .is_some_and(|code| code == "23505")
}

fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Database(error.to_string())
}

/// Deterministic test double. The Postgres adapter must preserve the outcomes
/// exposed by [`PackageRepository`], including checksum idempotency.
#[derive(Debug, Default)]
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
        self.packages_by_name
            .insert(package.name.clone(), package.clone());
        Ok(package)
    }

    fn find_package(&self, name: &str) -> Option<&Package> {
        self.packages_by_name.get(name)
    }

    fn publish_version(&mut self, request: PublishVersion) -> Result<PublishOutcome, StoreError> {
        validate_version(&request.version)?;
        validate_checksum(&request.checksum_sha256)?;
        if !self
            .packages_by_name
            .values()
            .any(|package| package.id == request.package_id)
        {
            return Err(StoreError::PackageNotFound);
        }
        let key = (request.package_id.clone(), request.version.clone());
        if let Some(existing) = self.versions_by_key.get(&key) {
            return if existing
                .checksum_sha256
                .eq_ignore_ascii_case(&request.checksum_sha256)
            {
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
        self.versions_by_key
            .get(&(package_id.to_owned(), version.to_owned()))
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
            return Err(if yanked {
                StoreError::VersionAlreadyYanked
            } else {
                StoreError::VersionNotYanked
            });
        }
        entity.is_yanked = yanked;
        entity.yanked_at_unix_seconds = yanked.then_some(now_unix_seconds);
        Ok(entity.clone())
    }
}

fn validate_package_name(name: &str) -> Result<(), StoreError> {
    (!name.trim().is_empty() && name == name.trim())
        .then_some(())
        .ok_or(StoreError::InvalidPackageName)
}

fn validate_subject(subject: &str) -> Result<(), StoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(StoreError::InvalidAuthHubSubject)
}

fn validate_version(version: &str) -> Result<(), StoreError> {
    (!version.trim().is_empty() && version == version.trim())
        .then_some(())
        .ok_or(StoreError::InvalidVersion)
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
            repository.create_package(NewPackage {
                owner_subject: "identity-user-id".into(),
                ..package_request()
            }),
            Err(StoreError::InvalidAuthHubSubject)
        );
    }

    #[test]
    fn package_names_are_unique() {
        let mut repository = InMemoryPackageRepository::default();
        repository.create_package(package_request()).unwrap();
        assert_eq!(
            repository.create_package(package_request()),
            Err(StoreError::PackageAlreadyExists)
        );
    }

    #[test]
    fn publish_is_idempotent_only_for_matching_checksum() {
        let mut repository = InMemoryPackageRepository::default();
        repository.create_package(package_request()).unwrap();
        assert!(matches!(
            repository.publish_version(publish_request()),
            Ok(PublishOutcome::Created(_))
        ));
        assert!(matches!(
            repository.publish_version(publish_request()),
            Ok(PublishOutcome::AlreadyExists(_))
        ));
        assert_eq!(
            repository.publish_version(PublishVersion {
                checksum_sha256: "f".repeat(64),
                ..publish_request()
            }),
            Err(StoreError::VersionImmutable)
        );
    }

    #[test]
    fn yanking_is_reversible_but_state_transitions_are_not_idempotent() {
        let mut repository = InMemoryPackageRepository::default();
        repository.create_package(package_request()).unwrap();
        repository.publish_version(publish_request()).unwrap();
        let yanked = repository
            .set_yanked("package-1", "1.0.0", true, 300)
            .unwrap();
        assert_eq!(yanked.yanked_at_unix_seconds, Some(300));
        assert_eq!(
            repository.set_yanked("package-1", "1.0.0", true, 301),
            Err(StoreError::VersionAlreadyYanked)
        );
        let restored = repository
            .set_yanked("package-1", "1.0.0", false, 302)
            .unwrap();
        assert_eq!(restored.yanked_at_unix_seconds, None);
    }

    #[test]
    fn migration_has_database_enforced_immutability_keys() {
        assert!(migrations::CREATE_PACKAGE_REGISTRY.contains("UNIQUE (name)"));
        assert!(migrations::CREATE_PACKAGE_REGISTRY.contains("UNIQUE (package_id, version)"));
        assert!(migrations::BACKFILL_REQUIRES_SUBJECT_MAPPING.contains("Do not infer subjects"));
    }
}
