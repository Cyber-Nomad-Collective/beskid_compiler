use beskid_pckg_auth::{
    ApiKeyIdentity, ApiKeyScope, ApiKeyVerifier, AuthHubHandoffClaims, AuthHubHandoffVerifier,
    AuthHubIdentity, AuthorizationError, HandoffRequest, Hs256AuthHubHandoffVerifier,
    PermissionGrant, Principal, ResourceAction, ResourceVisibility, SubjectRole,
    authorize_resource_access, issue_pckg_session, sign_auth_hub_handoff, verify_pckg_session,
};

#[test]
fn handoff_verifier_rejects_non_pckg_audiences() {
    let verifier = beskid_pckg_auth::RejectingAuthHubHandoffVerifier;

    let result = verifier.verify(HandoffRequest {
        app: "tracker".to_owned(),
        handoff: "opaque-handoff".to_owned(),
    });

    assert!(result.is_err());
}

#[test]
fn handoff_and_session_reject_ambiguous_legacy_subjects() {
    let service_token = "auth-hub-handoff-test-secret";
    let claims = AuthHubHandoffClaims {
        app: "pckg".to_owned(),
        subject: "legacy-identity-7".to_owned(),
        login: "octocat".to_owned(),
        sid: "hub-session-1".to_owned(),
        expires_at: 4_102_444_800,
    };
    let handoff = sign_auth_hub_handoff(&claims, service_token).expect("test handoff signs");
    let verifier = Hs256AuthHubHandoffVerifier::new(service_token).expect("test secret is valid");
    assert_eq!(
        verifier.verify(HandoffRequest {
            app: "pckg".to_owned(),
            handoff,
        }),
        Err(beskid_pckg_auth::AuthError::Rejected)
    );

    let session = issue_pckg_session(
        &AuthHubIdentity {
            subject: "legacy-identity-7".to_owned(),
            github_login: "octocat".to_owned(),
            hub_session_id: "hub-session-1".to_owned(),
        },
        service_token,
    )
    .expect("test session signs");
    assert_eq!(
        verify_pckg_session(&session, service_token),
        Err(beskid_pckg_auth::AuthError::Rejected)
    );
}

#[test]
fn api_key_verifier_is_an_injectable_boundary() {
    struct AcceptingVerifier;

    impl ApiKeyVerifier for AcceptingVerifier {
        fn verify(
            &self,
            raw_key: &str,
        ) -> Result<beskid_pckg_auth::ApiKeyIdentity, beskid_pckg_auth::AuthError> {
            Ok(beskid_pckg_auth::ApiKeyIdentity {
                key_id: "key-1".to_owned(),
                subject: raw_key.to_owned(),
                scopes: vec!["packages:write".to_owned()],
            })
        }
    }

    let identity = AcceptingVerifier
        .verify("bpk_test_key")
        .expect("test verifier accepts key");

    assert_eq!(identity.subject, "bpk_test_key");
    assert_eq!(identity.scopes, ["packages:write"]);
}

#[test]
fn api_key_scope_is_checked_by_the_storage_neutral_verifier_boundary() {
    struct PublishOnlyVerifier;

    impl ApiKeyVerifier for PublishOnlyVerifier {
        fn verify(&self, _raw_key: &str) -> Result<ApiKeyIdentity, beskid_pckg_auth::AuthError> {
            Ok(ApiKeyIdentity {
                key_id: "key-1".to_owned(),
                subject: "github:42".to_owned(),
                scopes: vec!["publish".to_owned()],
            })
        }
    }

    assert!(
        PublishOnlyVerifier
            .verify_scoped("bpk_test_key", ApiKeyScope::Publish)
            .is_ok()
    );
    assert_eq!(
        PublishOnlyVerifier
            .verify_scoped("bpk_test_key", ApiKeyScope::Read)
            .unwrap_err(),
        beskid_pckg_auth::AuthError::InsufficientScope
    );
}

#[test]
fn private_resources_hide_their_existence_from_non_owners() {
    let outsider = Principal::from_auth_hub(
        AuthHubIdentity {
            subject: "github:7".to_owned(),
            github_login: "outsider".to_owned(),
            hub_session_id: "session".to_owned(),
        },
        [SubjectRole::User],
    );

    assert_eq!(
        authorize_resource_access(
            Some(&outsider),
            "github:42",
            ResourceVisibility::Private,
            ResourceAction::Read,
            [],
        ),
        Err(AuthorizationError::NotFound)
    );
}

#[test]
fn owner_permission_grant_and_super_admin_are_authorized_for_mutations() {
    let owner = Principal::from_subject("github:42", [SubjectRole::User]);
    let moderator = Principal::from_subject("github:7", [SubjectRole::Moderator]);
    let admin = Principal::from_subject("github:1", [SubjectRole::SuperAdmin]);

    assert!(
        authorize_resource_access(
            Some(&owner),
            "github:42",
            ResourceVisibility::Private,
            ResourceAction::Publish,
            [],
        )
        .is_ok()
    );
    assert!(
        authorize_resource_access(
            Some(&moderator),
            "github:42",
            ResourceVisibility::Private,
            ResourceAction::Moderate,
            [PermissionGrant::new("github:7", ResourceAction::Moderate)],
        )
        .is_ok()
    );
    assert!(
        authorize_resource_access(
            Some(&admin),
            "github:42",
            ResourceVisibility::Private,
            ResourceAction::Manage,
            [],
        )
        .is_ok()
    );
}

#[test]
fn mutation_without_a_principal_is_unauthorized_and_non_owner_is_forbidden() {
    let outsider = Principal::from_subject("github:7", [SubjectRole::User]);

    assert_eq!(
        authorize_resource_access(
            None,
            "github:42",
            ResourceVisibility::Private,
            ResourceAction::Publish,
            [],
        ),
        Err(AuthorizationError::Unauthorized)
    );
    assert_eq!(
        authorize_resource_access(
            Some(&outsider),
            "github:42",
            ResourceVisibility::Private,
            ResourceAction::Publish,
            [],
        ),
        Err(AuthorizationError::Forbidden)
    );
}
