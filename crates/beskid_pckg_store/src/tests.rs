use super::{
    InMemoryPackageRepository, NewPackage, PackageRepository, PublishOutcome, PublishVersion, StoreError, migrations,
};

const CHECKSUM: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn package_request() -> NewPackage {
    NewPackage {
        id: "package-1".into(),
        name: "beskid.demo".into(),
        owner_subject: "octocat".into(),
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
fn package_owner_is_a_stable_authelia_subject() {
    let mut repository = InMemoryPackageRepository::default();
    let package = repository.create_package(package_request()).unwrap();
    assert_eq!(package.owner_subject, "octocat");
    assert_eq!(
        repository.create_package(NewPackage { owner_subject: "identity user".into(), ..package_request() }),
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
}

#[test]
fn package_review_queue_migration_retains_subjects_and_valid_actions() {
    assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("pckg_package_review_requests"));
    assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("'pending', 'approved', 'needs_changes', 'rejected'"));
    assert!(migrations::CREATE_PACKAGE_REVIEW_QUEUE.contains("reviewer_subject"));
}

#[test]
fn administration_migration_drops_role_storage_and_keeps_publisher_verification() {
    assert!(!migrations::CREATE_ADMINISTRATION.contains("pckg_admin_roles"));
    assert!(migrations::CREATE_ADMINISTRATION.contains("pckg_publisher_verifications"));
    assert!(migrations::CREATE_ADMINISTRATION.contains("pckg_resource_permissions"));
    assert!(migrations::CREATE_ADMINISTRATION.contains("resource_kind = 'package'"));
}

#[test]
fn subject_check_constraints_accept_authelia_and_github_subjects() {
    for migration in [
        migrations::CREATE_API_KEYS,
        migrations::CREATE_ADMINISTRATION,
        migrations::CREATE_PACKAGE_REVIEW_QUEUE,
        migrations::CREATE_REGISTRY_OPERATIONS,
        migrations::CREATE_PACKAGE_COMMUNITY_REVIEWS,
    ] {
        assert!(migration.contains("^[A-Za-z0-9._:@/-]+$"), "migration must accept Authelia subjects");
        assert!(!migration.contains("'^github:[0-9]+$'"), "migration must not require github-only subjects");
    }
}
