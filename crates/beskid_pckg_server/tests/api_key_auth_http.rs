mod support;

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode},
};
use beskid_pckg_server::{PckgServerConfig, router};
use beskid_pckg_store::{ApiKey, ApiKeyStoreError, AsyncApiKeyRepository, NewApiKey};
use support::{artifact, multipart_publish_request};
use tower::ServiceExt;

struct ActiveKeyRepository {
    active_token: String,
    key: ApiKey,
}

#[async_trait]
impl AsyncApiKeyRepository for ActiveKeyRepository {
    async fn create_api_key(&self, _request: NewApiKey) -> Result<ApiKey, ApiKeyStoreError> {
        Err(ApiKeyStoreError::InvalidToken)
    }

    async fn list_api_keys(&self, _subject: &str) -> Result<Vec<ApiKey>, ApiKeyStoreError> {
        Err(ApiKeyStoreError::InvalidToken)
    }

    async fn revoke_api_key(
        &self,
        _id: &str,
        _subject: &str,
        _now_unix_seconds: i64,
    ) -> Result<bool, ApiKeyStoreError> {
        Err(ApiKeyStoreError::InvalidToken)
    }

    async fn find_active_api_key_by_token(&self, raw_token: &str) -> Result<Option<ApiKey>, ApiKeyStoreError> {
        Ok((raw_token == self.active_token).then(|| self.key.clone()))
    }
}

fn bearer(value: &str) -> String {
    format!("Bearer {value}")
}

fn active_key_config() -> (PckgServerConfig, String) {
    let token = format!("bpk_{}", "a".repeat(64));
    let key = test_key(vec!["publish".to_owned()], None);
    (config_for_key(PckgServerConfig::default().with_authelia_auth(), &token, key), token)
}

fn test_key(scopes: Vec<String>, revoked_at_unix_seconds: Option<i64>) -> ApiKey {
    ApiKey {
        id: "00000000-0000-0000-0000-000000000001".to_owned(),
        subject: "key-owner".to_owned(),
        label: "CI".to_owned(),
        scopes,
        created_at_unix_seconds: 0,
        revoked_at_unix_seconds,
    }
}

fn config_for_key(config: PckgServerConfig, token: &str, key: ApiKey) -> PckgServerConfig {
    config.with_api_key_repository(Arc::new(ActiveKeyRepository { active_token: token.to_owned(), key }))
}

fn package_create_request(name: &str) -> Request<Body> {
    Request::post("/api/packages")
        .header("content-type", "application/json")
        .body(Body::from(format!(r#"{{"name":"{name}","isPublic":false,"submitForReview":false}}"#)))
        .unwrap()
}

fn with_authorization(mut request: Request<Body>, authorization: &str, remote_subject: Option<&str>) -> Request<Body> {
    request.headers_mut().insert("authorization", HeaderValue::from_str(authorization).unwrap());
    if let Some(remote_subject) = remote_subject {
        request.headers_mut().insert("remote-user", HeaderValue::from_str(remote_subject).unwrap());
    }
    request
}

#[tokio::test]
async fn active_bearer_api_key_owns_cli_package_mutations_without_remote_header_trust() {
    let (config, token) = active_key_config();
    let response = router(config)
        .oneshot(with_authorization(package_create_request("Key.Owner"), &bearer(&token), Some("header-attacker")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = http_body_util::BodyExt::collect(response.into_body()).await.unwrap().to_bytes();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["ownerUserId"], "key-owner");
}

#[tokio::test]
async fn active_bearer_api_key_publishes_without_browser_session_auth() {
    let token = format!("bpk_{}", "a".repeat(64));
    let config = config_for_key(PckgServerConfig::default(), &token, test_key(vec!["publish".to_owned()], None));
    let response = router(config)
        .oneshot(with_authorization(package_create_request("Key.NoSession"), &bearer(&token), Some("attacker")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn no_session_auth_rejects_absent_or_non_bearer_credentials() {
    let token = format!("bpk_{}", "a".repeat(64));
    let config = config_for_key(PckgServerConfig::default(), &token, test_key(vec!["publish".to_owned()], None));
    let app = router(config);
    let requests = [
        package_create_request("Key.NoCredentials"),
        with_authorization(package_create_request("Key.Basic"), "Basic ignored", Some("attacker")),
        {
            let mut request = package_create_request("Key.LegacyHeader");
            request.headers_mut().insert("x-api-key", HeaderValue::from_static("bpk_legacy"));
            request.headers_mut().insert("remote-user", HeaderValue::from_static("attacker"));
            request
        },
    ];

    for request in requests {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn inactive_bearer_key_cannot_fall_back_to_remote_user_headers() {
    let (config, _token) = active_key_config();
    let inactive = format!("bpk_{}", "b".repeat(64));
    let response = router(config)
        .oneshot(with_authorization(
            package_create_request("Key.Rejected"),
            &bearer(&inactive),
            Some("header-attacker"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn malformed_bearer_key_cannot_fall_back_to_remote_user_headers() {
    let (config, _token) = active_key_config();
    let response = router(config)
        .oneshot(with_authorization(
            package_create_request("Key.Malformed"),
            "Bearer bpk_not-a-valid-issued-token",
            Some("header-attacker"),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_key_without_publish_scope_cannot_mutate_packages() {
    let token = format!("bpk_{}", "c".repeat(64));
    let config = config_for_key(
        PckgServerConfig::default().with_authelia_auth(),
        &token,
        test_key(vec!["read".to_owned()], None),
    );
    let response = router(config)
        .oneshot(with_authorization(package_create_request("Key.ReadOnly"), &bearer(&token), None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn revoked_bearer_key_cannot_mutate_packages() {
    let token = format!("bpk_{}", "d".repeat(64));
    let config = config_for_key(
        PckgServerConfig::default().with_authelia_auth(),
        &token,
        test_key(vec!["publish".to_owned()], Some(1)),
    );
    let response = router(config)
        .oneshot(with_authorization(package_create_request("Key.Revoked"), &bearer(&token), None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn authorization_header_never_escalates_to_the_mock_principal() {
    let unknown_key = format!("bpk_{}", "e".repeat(64));
    let response = router(PckgServerConfig::default().with_mock_auth())
        .oneshot(with_authorization(package_create_request("Key.MockRejected"), &bearer(&unknown_key), None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn every_publication_mutation_rejects_an_invalid_bearer_before_remote_identity() {
    let (config, _token) = active_key_config();
    let invalid = bearer(&format!("bpk_{}", "f".repeat(64)));
    let app = router(config);
    let mut publish_request =
        multipart_publish_request("Key.Inventory", "1.0.0", "attacker", artifact("Key.Inventory", "1.0.0"));
    publish_request.headers_mut().insert("authorization", HeaderValue::from_str(&invalid).unwrap());
    let requests = [
        with_authorization(package_create_request("Key.Inventory"), &invalid, Some("attacker")),
        publish_request,
        Request::post("/api/packages/Key.Inventory/versions/1.0.0/yank")
            .header("authorization", &invalid)
            .header("remote-user", "attacker")
            .body(Body::empty())
            .unwrap(),
        Request::post("/api/packages/Key.Inventory/versions/1.0.0/unyank")
            .header("authorization", &invalid)
            .header("remote-user", "attacker")
            .body(Body::empty())
            .unwrap(),
    ];

    for request in requests {
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
