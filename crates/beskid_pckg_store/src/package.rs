use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::sql;

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
            Err(error) if sql::is_unique_violation(&error) => Err(StoreError::PackageAlreadyExists),
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


pub(super) fn validate_package_name(name: &str) -> Result<(), StoreError> {
    (!name.trim().is_empty() && name == name.trim()).then_some(()).ok_or(StoreError::InvalidPackageName)
}

pub(super) fn validate_subject(subject: &str) -> Result<(), StoreError> {
    (subject.starts_with("github:") && subject["github:".len()..].parse::<u64>().is_ok())
        .then_some(())
        .ok_or(StoreError::InvalidAuthHubSubject)
}

pub(super) fn validate_version(version: &str) -> Result<(), StoreError> {
    (!version.trim().is_empty() && version == version.trim()).then_some(()).ok_or(StoreError::InvalidVersion)
}

pub(super) fn validate_checksum(checksum: &str) -> Result<(), StoreError> {
    (checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(())
        .ok_or(StoreError::InvalidChecksum)
}

pub(super) fn parse_identifier(value: &str) -> Result<Uuid, StoreError> {
    sql::parse_uuid(value).ok_or(StoreError::InvalidIdentifier)
}

pub(super) fn as_u64(value: i64) -> Result<u64, StoreError> {
    sql::nonnegative_u64(value).ok_or(StoreError::InvalidIdentifier)
}

pub(super) fn timestamp(value: i64) -> Result<DateTime<Utc>, StoreError> {
    sql::utc_timestamp(value).ok_or(StoreError::InvalidIdentifier)
}

pub(super) fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Database(sql::database_message(error))
}
