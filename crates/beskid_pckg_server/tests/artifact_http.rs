use std::io::Write;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_auth::{AuthHubIdentity, issue_pckg_session};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use zip::write::SimpleFileOptions;

fn config(root: &std::path::Path) -> PckgServerConfig {
    PckgServerConfig::with_auth_secrets("auth-hub-test-secret", "pckg-session-test-secret")
        .with_artifact_root(root)
}

fn session(subject: &str) -> String {
    issue_pckg_session(
        &AuthHubIdentity {
            subject: subject.to_owned(),
            github_login: "octocat".to_owned(),
            hub_session_id: "hub-1".to_owned(),
        },
        "pckg-session-test-secret",
    )
    .expect("test session issues")
}

fn artifact(name: &str, version: &str) -> Vec<u8> {
    let manifest =
        format!(r#"{{"schema":"beskid.package.v1","id":"{name}","version":"{version}"}}"#);
    let project = format!("name = \"{name}\"\n");
    let source = "module Main\n";
    let checksums = [
        ("package.json", manifest.as_bytes()),
        ("Project.proj", project.as_bytes()),
        ("src/main.bsk", source.as_bytes()),
    ]
    .into_iter()
    .map(|(path, bytes)| format!("{}  {path}", hex_sha256(bytes)))
    .collect::<Vec<_>>()
    .join("\n");

    let mut output = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default();
    for (path, contents) in [
        ("package.json", manifest.as_bytes()),
        ("Project.proj", project.as_bytes()),
        ("src/main.bsk", source.as_bytes()),
        ("checksums.sha256", checksums.as_bytes()),
    ] {
        zip.start_file(path, options).expect("entry starts");
        zip.write_all(contents).expect("entry writes");
    }
    zip.finish().expect("zip finishes");
    output.into_inner()
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body is readable")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("response is JSON")
}

#[tokio::test]
async fn owner_can_upload_and_public_can_download_a_verified_artifact() {
    let root = std::env::temp_dir().join(format!("pckg-artifact-http-{}", std::process::id()));
    let app = router(config(&root));
    let owner = format!("pckg_session={}", session("github:1"));
    let bytes = artifact("Public.Demo", "1.0.0");

    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("cookie", &owner)
                .body(Body::from(r#"{"name":"Public.Demo","isPublic":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let uploaded = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Public.Demo/versions/1.0.0/artifact")
                .header("content-type", "application/vnd.beskid.package")
                .header("cookie", &owner)
                .body(Body::from(bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::CREATED);
    assert_eq!(json(uploaded).await["checksumSha256"], hex_sha256(&bytes));

    let downloaded = app
        .oneshot(
            Request::get("/api/packages/Public.Demo/versions/1.0.0/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(downloaded.status(), StatusCode::OK);
    assert_eq!(
        downloaded.headers().get("content-type").unwrap(),
        "application/vnd.beskid.package"
    );
    assert_eq!(
        downloaded.into_body().collect().await.unwrap().to_bytes(),
        bytes
    );

    std::fs::remove_dir_all(root).expect("artifact root is removed");
}

#[tokio::test]
async fn private_and_yanked_artifacts_are_hidden_from_non_owners_and_downloads() {
    let root = std::env::temp_dir().join(format!("pckg-artifact-private-{}", std::process::id()));
    let app = router(config(&root));
    let owner = format!("pckg_session={}", session("github:1"));
    let other = format!("pckg_session={}", session("github:2"));
    let bytes = artifact("Private.Demo", "1.0.0");

    for request in [
        Request::post("/api/packages")
            .header("content-type", "application/json")
            .header("cookie", &owner)
            .body(Body::from(r#"{"name":"Private.Demo","isPublic":false}"#))
            .unwrap(),
        Request::post("/api/packages/Private.Demo/versions/1.0.0/artifact")
            .header("cookie", &owner)
            .body(Body::from(bytes))
            .unwrap(),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());
    }

    let hidden = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Private.Demo/versions/1.0.0/download")
                .header("cookie", &other)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);

    let yanked = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Private.Demo/versions/1.0.0/yank")
                .header("cookie", &owner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(yanked.status(), StatusCode::OK);

    let hidden_after_yank = app
        .oneshot(
            Request::get("/api/packages/Private.Demo/versions/1.0.0/download")
                .header("cookie", &owner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden_after_yank.status(), StatusCode::NOT_FOUND);

    std::fs::remove_dir_all(root).expect("artifact root is removed");
}
