use std::fs;
use std::path::PathBuf;

use beskid_pckg::config::PckgClientConfig;
use beskid_pckg::error::PckgError;
use beskid_pckg::PckgClient;

#[tokio::test]
async fn list_versions_without_auth_returns_error() {
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

    let path = PathBuf::from(std::env::temp_dir()).join(format!(
        "beskid_pckg_test_{}.bpk",
        std::process::id()
    ));
    fs::write(&path, [1u8, 2, 3, 4]).expect("write artifact");

    let err = client
        .publish_package_version(
            "Demo",
            None,
            &path,
            "demo.bpk",
            Some("{}"),
            None,
            None,
        )
        .await
        .expect_err("missing auth should fail before network call");

    let _ = fs::remove_file(&path);
    assert!(matches!(err, PckgError::MissingAuthToken));
}
