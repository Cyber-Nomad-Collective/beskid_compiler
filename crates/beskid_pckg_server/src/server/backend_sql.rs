use beskid_pckg_store::{Package, PackageVersion, SqlxPackageRepository, StoreError};
use sqlx::Row;

fn row_package(row: sqlx::postgres::PgRow) -> Result<Package, StoreError> {
    Ok(Package {
        id: row.try_get("id").map_err(|error| StoreError::Database(error.to_string()))?,
        name: row.try_get("name").map_err(|error| StoreError::Database(error.to_string()))?,
        owner_subject: row.try_get("owner_subject").map_err(|error| StoreError::Database(error.to_string()))?,
        is_public: row.try_get("is_public").map_err(|error| StoreError::Database(error.to_string()))?,
        created_at_unix_seconds: row.try_get("created_at").map_err(|error| StoreError::Database(error.to_string()))?,
        updated_at_unix_seconds: row.try_get("updated_at").map_err(|error| StoreError::Database(error.to_string()))?,
    })
}

fn row_version(row: sqlx::postgres::PgRow) -> Result<PackageVersion, StoreError> {
    let size_bytes: i64 = row.try_get("size_bytes").map_err(|error| StoreError::Database(error.to_string()))?;
    Ok(PackageVersion {
        id: row.try_get("id").map_err(|error| StoreError::Database(error.to_string()))?,
        package_id: row.try_get("package_id").map_err(|error| StoreError::Database(error.to_string()))?,
        version: row.try_get("version").map_err(|error| StoreError::Database(error.to_string()))?,
        checksum_sha256: row.try_get("checksum_sha256").map_err(|error| StoreError::Database(error.to_string()))?,
        storage_key: row.try_get("storage_key").map_err(|error| StoreError::Database(error.to_string()))?,
        size_bytes: size_bytes.try_into().map_err(|_| StoreError::InvalidIdentifier)?,
        is_yanked: row.try_get("is_yanked").map_err(|error| StoreError::Database(error.to_string()))?,
        published_at_unix_seconds: row
            .try_get("published_at")
            .map_err(|error| StoreError::Database(error.to_string()))?,
        yanked_at_unix_seconds: row.try_get("yanked_at").map_err(|error| StoreError::Database(error.to_string()))?,
    })
}

const PACKAGE_SELECT: &str = "SELECT id::text AS id, name, owner_subject, is_public, EXTRACT(EPOCH FROM created_at_utc)::bigint AS created_at, EXTRACT(EPOCH FROM updated_at_utc)::bigint AS updated_at FROM pckg_packages";

const VERSION_SELECT: &str = "SELECT id::text AS id, package_id::text AS package_id, version, checksum_sha256, storage_key, size_bytes, is_yanked, EXTRACT(EPOCH FROM published_at_utc)::bigint AS published_at, EXTRACT(EPOCH FROM yanked_at_utc)::bigint AS yanked_at FROM pckg_package_versions";

pub(super) async fn sqlx_find_package_by_id(
    repository: &SqlxPackageRepository,
    id: &str,
) -> Result<Option<Package>, StoreError> {
    let query = format!("{PACKAGE_SELECT} WHERE id::text = $1");
    sqlx::query(&query)
        .bind(id)
        .fetch_optional(repository.pool())
        .await
        .map_err(|error| StoreError::Database(error.to_string()))?
        .map(row_package)
        .transpose()
}

pub(super) async fn sqlx_list_packages(
    repository: &SqlxPackageRepository,
    limit: i64,
    offset: i64,
) -> Result<Vec<Package>, StoreError> {
    let query = format!("{PACKAGE_SELECT} ORDER BY updated_at_utc DESC, name ASC LIMIT $1 OFFSET $2");
    sqlx::query(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(repository.pool())
        .await
        .map_err(|error| StoreError::Database(error.to_string()))?
        .into_iter()
        .map(row_package)
        .collect()
}

pub(super) async fn sqlx_list_versions(
    repository: &SqlxPackageRepository,
    package_id: &str,
) -> Result<Vec<PackageVersion>, StoreError> {
    let query = format!("{VERSION_SELECT} WHERE package_id::text = $1 ORDER BY published_at_utc DESC, version DESC");
    sqlx::query(&query)
        .bind(package_id)
        .fetch_all(repository.pool())
        .await
        .map_err(|error| StoreError::Database(error.to_string()))?
        .into_iter()
        .map(row_version)
        .collect()
}
