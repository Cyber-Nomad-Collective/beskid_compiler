use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use beskid_pckg_store::{
    AsyncPackageCommunityReviewRepository, AsyncPackageRepository, InMemoryPackageRepository, NewPackage, Package,
    PackageCommunityReview, PackageCommunityReviewError, PackageRepository, PackageVersion, PublishOutcome,
    PublishVersion, SqlxPackageRepository, StoreError, WorkspacePublishOutcome, WorkspacePublishReservation,
};

use super::backend_sql::{sqlx_find_package_by_id, sqlx_list_packages, sqlx_list_versions};

/// Storage is selected exactly once during startup. In-memory storage remains
/// intentionally available for isolated HTTP tests and local UI work without a
/// database; a configured database always goes through the SQLx boundary.
#[derive(Clone)]
pub(crate) enum PackageBackend {
    InMemory(Arc<InMemoryPackageBackend>),
    Sqlx(Arc<SqlxPackageRepository>),
}

/// Server-owned indexes make read enumeration available to the intentionally
/// minimal in-memory repository. PostgreSQL reads query the canonical tables
/// directly, so this is only a deterministic test/local adapter.
#[derive(Default)]
pub(crate) struct InMemoryPackageBackend {
    repository: std::sync::Mutex<InMemoryPackageRepository>,
    package_names: std::sync::Mutex<BTreeSet<String>>,
    versions_by_package: std::sync::Mutex<BTreeMap<String, BTreeSet<String>>>,
    community_reviews: std::sync::Mutex<Vec<PackageCommunityReview>>,
}

impl PackageBackend {
    pub(crate) fn in_memory() -> Self {
        Self::InMemory(Arc::new(InMemoryPackageBackend::default()))
    }

    pub(crate) async fn create_package(&self, request: NewPackage) -> Result<Package, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let package = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned")
                    .create_package(request)?;
                repository
                    .package_names
                    .lock()
                    .expect("package catalog mutex is not poisoned")
                    .insert(package.name.clone());
                Ok(package)
            }
            Self::Sqlx(repository) => repository.create_package(request).await,
        }
    }

    pub(crate) async fn find_package(&self, name: &str) -> Result<Option<Package>, StoreError> {
        match self {
            Self::InMemory(repository) => Ok(repository
                .repository
                .lock()
                .expect("package repository mutex is not poisoned")
                .find_package(name)
                .cloned()),
            Self::Sqlx(repository) => repository.find_package(name).await,
        }
    }

    pub(crate) async fn delete_package(&self, name: &str) -> Result<Vec<PackageVersion>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let mut package_repository =
                    repository.repository.lock().expect("package repository mutex is not poisoned");
                let package_id = package_repository
                    .find_package(name)
                    .map(|package| package.id.clone())
                    .ok_or(StoreError::PackageNotFound)?;
                let versions = package_repository.delete_package(name)?;
                drop(package_repository);
                repository.package_names.lock().expect("package catalog mutex is not poisoned").remove(name);
                repository
                    .versions_by_package
                    .lock()
                    .expect("version catalog mutex is not poisoned")
                    .remove(&package_id);
                Ok(versions)
            }
            Self::Sqlx(repository) => repository.delete_package(name).await,
        }
    }

    pub(crate) async fn find_package_by_id(&self, id: &str) -> Result<Option<Package>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let names = repository.package_names.lock().expect("package catalog mutex is not poisoned").clone();
                let repository = repository.repository.lock().expect("package repository mutex is not poisoned");
                Ok(names
                    .into_iter()
                    .find_map(|name| repository.find_package(&name).filter(|package| package.id == id).cloned()))
            }
            Self::Sqlx(repository) => sqlx_find_package_by_id(repository, id).await,
        }
    }

    pub(crate) async fn list_packages(&self, limit: i64, offset: i64) -> Result<Vec<Package>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let names = repository.package_names.lock().expect("package catalog mutex is not poisoned").clone();
                let repository = repository.repository.lock().expect("package repository mutex is not poisoned");
                let mut packages =
                    names.into_iter().filter_map(|name| repository.find_package(&name).cloned()).collect::<Vec<_>>();
                packages.sort_by(|left, right| {
                    right
                        .updated_at_unix_seconds
                        .cmp(&left.updated_at_unix_seconds)
                        .then_with(|| left.name.cmp(&right.name))
                });
                Ok(packages.into_iter().skip(offset as usize).take(limit as usize).collect())
            }
            Self::Sqlx(repository) => sqlx_list_packages(repository, limit, offset).await,
        }
    }

    pub(crate) async fn list_versions(&self, package_id: &str) -> Result<Vec<PackageVersion>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let versions = repository
                    .versions_by_package
                    .lock()
                    .expect("version catalog mutex is not poisoned")
                    .get(package_id)
                    .cloned()
                    .unwrap_or_default();
                let repository = repository.repository.lock().expect("package repository mutex is not poisoned");
                let mut versions = versions
                    .into_iter()
                    .filter_map(|version| repository.find_version(package_id, &version).cloned())
                    .collect::<Vec<_>>();
                versions.sort_by(|left, right| {
                    right
                        .published_at_unix_seconds
                        .cmp(&left.published_at_unix_seconds)
                        .then_with(|| right.version.cmp(&left.version))
                });
                Ok(versions)
            }
            Self::Sqlx(repository) => sqlx_list_versions(repository, package_id).await,
        }
    }

    pub(crate) async fn publish_version(&self, request: PublishVersion) -> Result<PublishOutcome, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let outcome = repository
                    .repository
                    .lock()
                    .expect("package repository mutex is not poisoned")
                    .publish_version(request)?;
                let version = match &outcome {
                    PublishOutcome::Created(version) | PublishOutcome::AlreadyExists(version) => version,
                };
                repository
                    .versions_by_package
                    .lock()
                    .expect("version catalog mutex is not poisoned")
                    .entry(version.package_id.clone())
                    .or_default()
                    .insert(version.version.clone());
                Ok(outcome)
            }
            Self::Sqlx(repository) => repository.publish_version(request).await,
        }
    }

    /// Atomically reserves all metadata for a workspace. The in-memory
    /// implementation holds one repository lock and restores its snapshot on
    /// any error; PostgreSQL delegates to its explicit batch transaction.
    pub(crate) async fn publish_workspace_batch(
        &self,
        reservations: Vec<WorkspacePublishReservation>,
    ) -> Result<Vec<WorkspacePublishOutcome>, StoreError> {
        match self {
            Self::InMemory(repository) => {
                let mut package_repository =
                    repository.repository.lock().expect("package repository mutex is not poisoned");
                let before = package_repository.clone();
                let result = (|| {
                    let mut outcomes = Vec::with_capacity(reservations.len());
                    for reservation in &reservations {
                        let package = match package_repository.find_package(&reservation.package.name).cloned() {
                            Some(package) => {
                                if package.owner_subject != reservation.package.owner_subject {
                                    return Err(StoreError::PackageOwnershipConflict);
                                }
                                package
                            }
                            None => package_repository.create_package(reservation.package.clone())?,
                        };
                        let version = package_repository.publish_version(PublishVersion {
                            id: reservation.version_id.clone(),
                            package_id: package.id.clone(),
                            version: reservation.version.clone(),
                            checksum_sha256: reservation.checksum_sha256.clone(),
                            storage_key: reservation.storage_key.clone(),
                            size_bytes: reservation.size_bytes,
                            now_unix_seconds: reservation.package.now_unix_seconds,
                        })?;
                        outcomes.push(WorkspacePublishOutcome { package, version });
                    }
                    Ok(outcomes)
                })();
                if result.is_err() {
                    *package_repository = before;
                    return result;
                }
                let outcomes = result.expect("checked above");
                drop(package_repository);
                let mut names = repository.package_names.lock().expect("package catalog mutex is not poisoned");
                let mut versions =
                    repository.versions_by_package.lock().expect("version catalog mutex is not poisoned");
                for outcome in &outcomes {
                    names.insert(outcome.package.name.clone());
                    let version = match &outcome.version {
                        PublishOutcome::Created(version) | PublishOutcome::AlreadyExists(version) => version,
                    };
                    versions.entry(version.package_id.clone()).or_default().insert(version.version.clone());
                }
                Ok(outcomes)
            }
            Self::Sqlx(repository) => repository.publish_workspace_batch(&reservations).await,
        }
    }

    pub(crate) async fn find_version(
        &self,
        package_id: &str,
        version: &str,
    ) -> Result<Option<PackageVersion>, StoreError> {
        match self {
            Self::InMemory(repository) => Ok(repository
                .repository
                .lock()
                .expect("package repository mutex is not poisoned")
                .find_version(package_id, version)
                .cloned()),
            Self::Sqlx(repository) => repository.find_version(package_id, version).await,
        }
    }

    pub(crate) async fn upsert_community_review(
        &self,
        review: PackageCommunityReview,
    ) -> Result<PackageCommunityReview, PackageCommunityReviewError> {
        match self {
            Self::Sqlx(repository) => repository.upsert_package_community_review(review).await,
            Self::InMemory(repository) => {
                if !(1..=5).contains(&review.rating) {
                    return Err(PackageCommunityReviewError::InvalidRating);
                }
                if review.comment.trim().is_empty() {
                    return Err(PackageCommunityReviewError::InvalidComment);
                }
                let mut reviews = repository.community_reviews.lock().expect("community reviews mutex is not poisoned");
                if let Some(existing) = reviews.iter_mut().find(|existing| {
                    existing.package_id == review.package_id && existing.author_subject == review.author_subject
                }) {
                    let mut updated = review;
                    updated.id = existing.id.clone();
                    *existing = updated.clone();
                    Ok(updated)
                } else {
                    reviews.push(review.clone());
                    Ok(review)
                }
            }
        }
    }

    pub(crate) async fn community_reviews(
        &self,
        package_id: &str,
    ) -> Result<Vec<PackageCommunityReview>, PackageCommunityReviewError> {
        match self {
            Self::Sqlx(repository) => repository.list_package_community_reviews(package_id).await,
            Self::InMemory(repository) => Ok(repository
                .community_reviews
                .lock()
                .expect("community reviews mutex is not poisoned")
                .iter()
                .filter(|review| review.package_id == package_id)
                .cloned()
                .collect()),
        }
    }

    pub(crate) async fn set_yanked(
        &self,
        package_id: &str,
        version: &str,
        yanked: bool,
        now_unix_seconds: i64,
    ) -> Result<PackageVersion, StoreError> {
        match self {
            Self::InMemory(repository) => repository
                .repository
                .lock()
                .expect("package repository mutex is not poisoned")
                .set_yanked(package_id, version, yanked, now_unix_seconds),
            Self::Sqlx(repository) => repository.set_yanked(package_id, version, yanked, now_unix_seconds).await,
        }
    }
}
