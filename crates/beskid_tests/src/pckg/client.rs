use std::time::Duration;

use beskid_pckg::config::PckgAuth;
use beskid_pckg::{PckgClient, PckgClientConfig, PckgError};

#[test]
fn config_defaults_to_no_auth_and_api_key_header() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    assert_eq!(config.auth, None);
    assert_eq!(config.api_key_header_name, "X-API-Key");
}

#[test]
fn config_supports_publisher_api_key_auth_mode() {
    let config = PckgClientConfig::new("http://localhost:5195")
        .expect("valid URL")
        .with_publisher_api_key("bpk_abc");

    assert_eq!(
        config.auth,
        Some(PckgAuth::PublisherApiKey("bpk_abc".to_string()))
    );
}

#[tokio::test]
async fn public_packages_endpoint_without_auth_attempts_request() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    let client = PckgClient::new(config).expect("client should build");

    let err = client
        .list_packages()
        .await
        .expect_err("request should fail due to absent local server");

    assert!(!matches!(err, PckgError::MissingAuthToken));
}

#[tokio::test]
async fn protected_endpoint_with_api_key_attempts_request() {
    let config = PckgClientConfig::new("http://127.0.0.1:9")
        .expect("valid URL")
        .with_timeout(Duration::from_millis(250))
        .with_publisher_api_key("bpk_test");
    let client = PckgClient::new(config).expect("client should build");

    let err = client
        .list_packages()
        .await
        .expect_err("request should fail due to unreachable server");

    assert!(!matches!(err, PckgError::MissingAuthToken));
}

#[tokio::test]
async fn protected_api_keys_endpoint_without_auth_returns_missing_auth_token() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    let client = PckgClient::new(config).expect("client should build");

    let err = client
        .list_api_keys()
        .await
        .expect_err("missing auth should fail before network call");

    assert!(matches!(err, PckgError::MissingAuthToken));
}

#[tokio::test]
async fn public_versions_endpoint_without_auth_attempts_request() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    let client = PckgClient::new(config).expect("client should build");

    let err = client
        .list_package_versions("Demo")
        .await
        .expect_err("request should fail due to absent local server");

    assert!(!matches!(err, PckgError::MissingAuthToken));
}

#[tokio::test]
async fn publish_endpoint_without_auth_returns_missing_auth_token() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    let client = PckgClient::new(config).expect("client should build");

    let err = client
        .publish_package_version(
            "Demo",
            "1.0.0",
            "demo.bpk",
            vec![1, 2, 3, 4],
            Some("{}"),
            None,
        )
        .await
        .expect_err("missing auth should fail before network call");

    assert!(matches!(err, PckgError::MissingAuthToken));
}
