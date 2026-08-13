use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::package::{
    SqlxPackageRepository, StoreError, as_u64, database_error, parse_identifier, timestamp, validate_subject,
};

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

impl SqlxPackageRepository {
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
        let mut transaction = self.pool().begin().await.map_err(database_error)?;

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

pub(super) fn validate_cutover_request(
    request: &LegacyIdentityCutoverRequest,
) -> Result<(), LegacyIdentityCutoverError> {
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
