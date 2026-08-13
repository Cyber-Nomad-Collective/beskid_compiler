use std::collections::BTreeMap;

use crate::package::{
    NewPackage, Package, PackageRepository, PackageVersion, PublishOutcome, PublishVersion, StoreError,
    validate_checksum, validate_package_name, validate_subject, validate_version,
};

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
