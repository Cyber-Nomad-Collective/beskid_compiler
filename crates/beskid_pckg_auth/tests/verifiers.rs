use beskid_pckg_auth::{ApiKeyVerifier, AuthHubHandoffVerifier, HandoffRequest};

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
