use beskid_pckg_auth::{
    ApiKeyIdentity, ApiKeyScope, ApiKeyVerifier, AuthError, AuthMode, AutheliaIdentity, AuthorizationError,
    PermissionGrant, Principal, ResourceAction, ResourceVisibility, SubjectRole, authorize_resource_access,
    is_valid_subject,
};

#[test]
fn auth_mode_parses_known_values_and_rejects_unknown_values() {
    assert_eq!(AuthMode::parse("mock"), Ok(AuthMode::Mock));
    assert_eq!(AuthMode::parse("authelia"), Ok(AuthMode::Authelia));
    assert_eq!(AuthMode::parse("oidc"), Err(AuthError::MissingConfiguration));
    assert_eq!(AuthMode::Mock.as_str(), "mock");
    assert_eq!(AuthMode::Authelia.as_str(), "authelia");
}

#[test]
fn principal_from_authelia_maps_groups_to_roles_and_always_grants_user() {
    let identity = AutheliaIdentity {
        subject: "octocat".to_owned(),
        email: Some("octocat@example.test".to_owned()),
        display_name: Some("Octocat".to_owned()),
        groups: vec!["pckg-admins".to_owned(), "pckg-moderators".to_owned()],
    };

    let principal = Principal::from_authelia(&identity, "pckg-admins", "pckg-moderators");
    assert_eq!(principal.subject(), "octocat");
    assert!(principal.has_role(SubjectRole::User));
    assert!(principal.has_role(SubjectRole::SuperAdmin));
    assert!(principal.has_role(SubjectRole::Moderator));

    let plain = Principal::from_authelia(
        &AutheliaIdentity { subject: "plain".to_owned(), email: None, display_name: None, groups: vec![] },
        "pckg-admins",
        "pckg-moderators",
    );
    assert!(plain.has_role(SubjectRole::User));
    assert!(!plain.has_role(SubjectRole::SuperAdmin));
}

#[test]
fn api_key_verifier_is_an_injectable_boundary() {
    struct AcceptingVerifier;

    impl ApiKeyVerifier for AcceptingVerifier {
        fn verify(&self, raw_key: &str) -> Result<ApiKeyIdentity, AuthError> {
            Ok(ApiKeyIdentity {
                key_id: "key-1".to_owned(),
                subject: raw_key.to_owned(),
                scopes: vec!["packages:write".to_owned()],
            })
        }
    }

    let identity = AcceptingVerifier.verify("bpk_test_key").expect("test verifier accepts key");
    assert_eq!(identity.subject, "bpk_test_key");
    assert_eq!(identity.scopes, ["packages:write"]);
}

#[test]
fn api_key_scope_is_checked_by_the_storage_neutral_verifier_boundary() {
    struct PublishOnlyVerifier;

    impl ApiKeyVerifier for PublishOnlyVerifier {
        fn verify(&self, _raw_key: &str) -> Result<ApiKeyIdentity, AuthError> {
            Ok(ApiKeyIdentity {
                key_id: "key-1".to_owned(),
                subject: "octocat".to_owned(),
                scopes: vec!["publish".to_owned()],
            })
        }
    }

    assert!(PublishOnlyVerifier.verify_scoped("bpk_test_key", ApiKeyScope::Publish).is_ok());
    assert_eq!(
        PublishOnlyVerifier.verify_scoped("bpk_test_key", ApiKeyScope::Read).unwrap_err(),
        AuthError::InsufficientScope
    );
}

#[test]
fn private_resources_hide_their_existence_from_non_owners() {
    let outsider = Principal::from_subject("outsider", [SubjectRole::User]);

    assert_eq!(
        authorize_resource_access(Some(&outsider), "owner", ResourceVisibility::Private, ResourceAction::Read, []),
        Err(AuthorizationError::NotFound)
    );
}

#[test]
fn owner_permission_grant_and_super_admin_are_authorized_for_mutations() {
    let owner = Principal::from_subject("owner", [SubjectRole::User]);
    let moderator = Principal::from_subject("mod", [SubjectRole::Moderator]);
    let admin = Principal::from_subject("admin", [SubjectRole::SuperAdmin]);

    assert!(
        authorize_resource_access(Some(&owner), "owner", ResourceVisibility::Private, ResourceAction::Publish, [])
            .is_ok()
    );
    assert!(
        authorize_resource_access(
            Some(&moderator),
            "owner",
            ResourceVisibility::Private,
            ResourceAction::Moderate,
            [PermissionGrant::new("mod", ResourceAction::Moderate)],
        )
        .is_ok()
    );
    assert!(
        authorize_resource_access(Some(&admin), "owner", ResourceVisibility::Private, ResourceAction::Manage, [])
            .is_ok()
    );
}

#[test]
fn mutation_without_a_principal_is_unauthorized_and_non_owner_is_forbidden() {
    let outsider = Principal::from_subject("outsider", [SubjectRole::User]);

    assert_eq!(
        authorize_resource_access(None, "owner", ResourceVisibility::Private, ResourceAction::Publish, []),
        Err(AuthorizationError::Unauthorized)
    );
    assert_eq!(
        authorize_resource_access(Some(&outsider), "owner", ResourceVisibility::Private, ResourceAction::Publish, []),
        Err(AuthorizationError::Forbidden)
    );
}

#[test]
fn is_valid_subject_accepts_authelia_and_github_subjects_and_rejects_garbage() {
    assert!(is_valid_subject("octocat"));
    assert!(is_valid_subject("github:42"));
    assert!(is_valid_subject("user.name"));
    assert!(!is_valid_subject(""));
    assert!(!is_valid_subject(" trimmed "));
    assert!(!is_valid_subject("has space"));
    assert!(!is_valid_subject(&"x".repeat(256)));
}
