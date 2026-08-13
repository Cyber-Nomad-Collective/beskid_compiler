use super::{
    InMemoryPackageRepository, LegacyIdentityCutoverError, LegacyIdentityCutoverRequest, LegacyIdentitySubjectMapping,
    NewPackage, PackageRepository, PublishOutcome, PublishVersion, StoreError, migrations, validate_cutover_request,
};
use crate::CREATE_TEST_NOTIFICATION_PROFILE_SQL;

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
