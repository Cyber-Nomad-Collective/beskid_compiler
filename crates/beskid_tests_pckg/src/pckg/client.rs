use std::{fs, io::Write, sync::Arc};

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use beskid_pckg::error::PckgError;
use beskid_pckg::{PckgClient, config::PckgClientConfig};
use beskid_pckg_server::{PckgServerConfig, router};
use beskid_pckg_store::{ApiKey, ApiKeyStoreError, AsyncApiKeyRepository, NewApiKey};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use zip::write::SimpleFileOptions;

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

fn package_artifact(name: &str, version: &str) -> Vec<u8> {
    let manifest =
        format!(r#"{{"schema":"beskid.package.v1","id":"{name}","version":"{version}","packageKind":"library"}}"#,);
    let project_name = name.replace('.', "_").to_ascii_lowercase();
    let project_manifest = format!("{project_name}.bproj");
    let project = format!("{project_name} {{\n  name = \"{project_name}\"\n}}\n");
    let source = "module Main\n";
    let entries = [
        ("package.json", manifest.as_bytes()),
        (project_manifest.as_str(), project.as_bytes()),
        ("src/main.bd", source.as_bytes()),
    ];
    let checksums = entries
        .iter()
        .map(|(path, bytes)| format!("{:x}  {path}", Sha256::digest(bytes)))
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default();
    for (path, bytes) in entries {
        zip.start_file(path, options).expect("artifact entry starts");
        zip.write_all(bytes).expect("artifact entry writes");
    }
    zip.start_file("checksums.sha256", options).expect("checksum entry starts");
    zip.write_all(checksums.as_bytes()).expect("checksum entry writes");
    zip.finish().expect("artifact finishes");
    output.into_inner()
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn multipart_publish_request(name: &str, version: &str, authorization: &str, artifact: Vec<u8>) -> Request<Body> {
    const BOUNDARY: &str = "beskid-pckg-client-test-boundary";
    let checksum = hex_sha256(&artifact);
    let mut body = Vec::new();
    for (field, value) in [("version", version.as_bytes()), ("checksumSha256", checksum.as_bytes())] {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{field}\"\r\n\r\n").as_bytes());
        body.extend_from_slice(value);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"artifact\"; filename=\"package.bpk\"\r\nContent-Type: application/zip\r\n\r\n",
    );
    body.extend_from_slice(&artifact);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());

    Request::post(format!("/api/packages/{name}/versions"))
        .header("content-type", format!("multipart/form-data; boundary={BOUNDARY}"))
        .header("authorization", authorization)
        .body(Body::from(body))
        .expect("build canonical package version request")
}

#[tokio::test]
async fn publisher_client_bearer_key_reaches_server_without_browser_session_auth() {
    let token = format!("bpk_{}", "a".repeat(64));
    let key = ApiKey {
        id: "00000000-0000-0000-0000-000000000001".to_owned(),
        subject: "key-owner".to_owned(),
        label: "CI".to_owned(),
        scopes: vec!["publish".to_owned()],
        created_at_unix_seconds: 0,
        revoked_at_unix_seconds: None,
    };
    let artifact_root = std::env::temp_dir().join(format!("pckg-bearer-client-{}", std::process::id()));
    let _ = fs::remove_dir_all(&artifact_root);
    let app = router(
        PckgServerConfig::default()
            .with_api_key_repository(Arc::new(ActiveKeyRepository { active_token: token.clone(), key }))
            .with_artifact_root(&artifact_root),
    );
    let package_name = "Key.ClientContract";
    let authorization = format!("Bearer {token}");
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("authorization", &authorization)
                .body(Body::from(format!(r#"{{"name":"{package_name}","isPublic":false,"submitForReview":false}}"#)))
                .expect("build package creation request"),
        )
        .await
        .expect("create package through local pckg router");
    assert_eq!(created.status(), StatusCode::CREATED);
    let version = app
        .clone()
        .oneshot(multipart_publish_request(
            package_name,
            "1.0.0",
            &authorization,
            package_artifact(package_name, "1.0.0"),
        ))
        .await
        .expect("create package version through local pckg router");
    assert_eq!(version.status(), StatusCode::CREATED);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind local pckg test server");
    let address = listener.local_addr().expect("read local pckg test address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve local pckg test router");
    });

    let client = PckgClient::new(
        PckgClientConfig::new(format!("http://{address}")).expect("valid local pckg URL").with_publisher_api_key(token),
    )
    .expect("build pckg publisher client");
    let result = client.yank_package_version(package_name, "1.0.0").await;

    server.abort();
    let _ = fs::remove_dir_all(&artifact_root);
    assert!(result.is_ok(), "configured publisher key must reach the server as a valid bearer: {result:?}");
}

#[tokio::test]
async fn publisher_client_uses_canonical_multipart_versions_contract() {
    let token = format!("bpk_{}", "c".repeat(64));
    let key = ApiKey {
        id: "00000000-0000-0000-0000-000000000003".to_owned(),
        subject: "package-owner".to_owned(),
        label: "Release".to_owned(),
        scopes: vec!["publish".to_owned()],
        created_at_unix_seconds: 0,
        revoked_at_unix_seconds: None,
    };
    let artifact_root = std::env::temp_dir().join(format!("pckg-client-contract-{}", std::process::id()));
    let app = router(
        PckgServerConfig::default()
            .with_api_key_repository(Arc::new(ActiveKeyRepository { active_token: token.clone(), key }))
            .with_artifact_root(&artifact_root),
    );
    let package_name = "Client.Contract";
    let authorization = format!("Bearer {token}");
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("authorization", &authorization)
                .body(Body::from(format!(r#"{{"name":"{package_name}","isPublic":false,"submitForReview":false}}"#)))
                .expect("build package creation request"),
        )
        .await
        .expect("create package through local pckg router");
    assert_eq!(created.status(), StatusCode::CREATED);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind local pckg test server");
    let address = listener.local_addr().expect("read local pckg test address");
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve local pckg test router");
    });
    let bytes = package_artifact(package_name, "1.2.3");
    let artifact_path = artifact_root.with_extension("bpk");
    fs::write(&artifact_path, &bytes).expect("write package artifact");
    let client = PckgClient::new(
        PckgClientConfig::new(format!("http://{address}")).expect("valid local pckg URL").with_publisher_api_key(token),
    )
    .expect("build pckg publisher client");

    let result = client
        .publish_package_version(package_name, &artifact_path, "client-contract.bpk", Some(&hex_sha256(&bytes)), None)
        .await;

    server.abort();
    let _ = fs::remove_file(&artifact_path);
    let _ = fs::remove_dir_all(&artifact_root);
    let response = result.expect("client and server share the canonical per-package publication contract");
    assert_eq!(response.version, "1.2.3");
}

#[tokio::test]
async fn obsolete_package_publish_route_is_not_exposed_by_the_rust_server() {
    let response = router(PckgServerConfig::default())
        .oneshot(
            Request::post("/api/packages/Client.Contract/publish")
                .body(Body::empty())
                .expect("build obsolete route request"),
        )
        .await
        .expect("route response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_versions_without_auth_returns_error() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    let client = PckgClient::new(config).expect("client should build");

    let err = client.list_package_versions("Demo").await.expect_err("request should fail due to absent local server");

    assert!(!matches!(err, PckgError::MissingAuthToken));
}

#[tokio::test]
async fn publish_endpoint_without_auth_returns_missing_auth_token() {
    let config = PckgClientConfig::new("http://localhost:5195").expect("valid URL");
    let client = PckgClient::new(config).expect("client should build");

    let path = std::env::temp_dir().join(format!("beskid_pckg_test_{}.bpk", std::process::id()));
    fs::write(&path, [1u8, 2, 3, 4]).expect("write artifact");

    let err = client
        .publish_package_version("Demo", &path, "demo.bpk", None, None)
        .await
        .expect_err("missing auth should fail before network call");

    let _ = fs::remove_file(&path);
    assert!(matches!(err, PckgError::MissingAuthToken));
}
