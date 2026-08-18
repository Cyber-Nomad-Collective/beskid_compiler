use std::io::Write;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use beskid_pckg_server::{PckgServerConfig, router};
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use zip::write::SimpleFileOptions;

fn config(root: &std::path::Path) -> PckgServerConfig {
    PckgServerConfig::default().with_authelia_auth().with_artifact_root(root)
}

fn artifact(name: &str, version: &str) -> Vec<u8> {
    let manifest = format!(r#"{{"schema":"beskid.package.v1","id":"{name}","version":"{version}"}}"#);
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

fn artifact_with_browsable_content(name: &str, version: &str) -> Vec<u8> {
    let manifest = format!(r#"{{"schema":"beskid.package.v1","id":"{name}","version":"{version}"}}"#);
    let project = format!("name = \"{name}\"\n");
    let entries = vec![
        ("package.json", manifest.into_bytes()),
        ("Project.proj", project.into_bytes()),
        ("README.md", b"# Public Demo\n".to_vec()),
        ("docs/guide.md", b"Use `public.demo`.\n".to_vec()),
        (".beskid/docs/metadata.json", br#"{"title":"Public Demo"}"#.to_vec()),
        ("src/main.bsk", b"module Main\n".to_vec()),
    ];
    let checksums =
        entries.iter().map(|(path, bytes)| format!("{}  {path}", hex_sha256(bytes))).collect::<Vec<_>>().join("\n");
    let mut output = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default();
    for (path, contents) in entries {
        zip.start_file(path, options).expect("entry starts");
        zip.write_all(&contents).expect("entry writes");
    }
    zip.start_file("checksums.sha256", options).expect("checksums entry starts");
    zip.write_all(checksums.as_bytes()).expect("checksums entry writes");
    zip.finish().expect("zip finishes");
    output.into_inner()
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.expect("body is readable").to_bytes();
    serde_json::from_slice(&bytes).expect("response is JSON")
}

fn authed(method: &str, uri: &str, subject: &str) -> Request<Body> {
    Request::builder().method(method).uri(uri).header("remote-user", subject).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn owner_can_upload_and_public_can_download_a_verified_artifact() {
    let root = std::env::temp_dir().join(format!("pckg-artifact-http-{}", std::process::id()));
    let app = router(config(&root));
    let bytes = artifact("Public.Demo", "1.0.0");

    let created = app
        .clone()
        .oneshot(
            Request::post("/api/packages")
                .header("content-type", "application/json")
                .header("remote-user", "owner")
                .body(Body::from(r#"{"name":"Public.Demo","isPublic":true,"submitForReview":false}"#))
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
                .header("remote-user", "owner")
                .body(Body::from(bytes.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    let uploaded_status = uploaded.status();
    assert_eq!(uploaded_status, StatusCode::CREATED, "{:?}", json(uploaded).await);
    assert_eq!(json(uploaded).await["checksumSha256"], hex_sha256(&bytes));

    let downloaded = app
        .clone()
        .oneshot(Request::get("/api/packages/Public.Demo/versions/1.0.0/download").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(downloaded.status(), StatusCode::OK);
    assert_eq!(downloaded.headers().get("content-type").unwrap(), "application/vnd.beskid.package");
    assert_eq!(downloaded.into_body().collect().await.unwrap().to_bytes(), bytes);

    let newer = artifact("Public.Demo", "2.0.0");
    let upload_newer = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Public.Demo/versions/2.0.0/artifact")
                .header("remote-user", "owner")
                .body(Body::from(newer.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload_newer.status(), StatusCode::CREATED);
    let yank_newer = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Public.Demo/versions/2.0.0/yank")
                .header("remote-user", "owner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(yank_newer.status(), StatusCode::OK);
    let latest = app
        .oneshot(Request::get("/api/packages/Public.Demo/versions/latest/download").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(latest.status(), StatusCode::OK);
    assert_eq!(latest.into_body().collect().await.unwrap().to_bytes(), bytes);

    std::fs::remove_dir_all(root).expect("artifact root is removed");
}

#[tokio::test]
async fn private_and_yanked_artifacts_are_hidden_from_non_owners_and_downloads() {
    let root = std::env::temp_dir().join(format!("pckg-artifact-private-{}", std::process::id()));
    let app = router(config(&root));
    let bytes = artifact("Private.Demo", "1.0.0");

    for request in [
        Request::post("/api/packages")
            .header("content-type", "application/json")
            .header("remote-user", "owner")
            .body(Body::from(r#"{"name":"Private.Demo","isPublic":false,"submitForReview":false}"#))
            .unwrap(),
        Request::post("/api/packages/Private.Demo/versions/1.0.0/artifact")
            .header("remote-user", "owner")
            .body(Body::from(bytes))
            .unwrap(),
    ] {
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(response.status().is_success());
    }

    let hidden = app
        .clone()
        .oneshot(authed("GET", "/api/packages/Private.Demo/versions/1.0.0/download", "other"))
        .await
        .unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);

    let yanked = app
        .clone()
        .oneshot(
            Request::post("/api/packages/Private.Demo/versions/1.0.0/yank")
                .header("remote-user", "owner")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(yanked.status(), StatusCode::OK);

    let hidden_after_yank =
        app.oneshot(authed("GET", "/api/packages/Private.Demo/versions/1.0.0/download", "owner")).await.unwrap();
    assert_eq!(hidden_after_yank.status(), StatusCode::NOT_FOUND);

    std::fs::remove_dir_all(root).expect("artifact root is removed");
}

#[tokio::test]
async fn public_package_artifact_browse_routes_expose_only_verified_docs_and_source() {
    let root = std::env::temp_dir().join(format!("pckg-artifact-browse-{}", std::process::id()));
    let app = router(config(&root));
    let artifact = artifact_with_browsable_content("Public.Demo", "1.0.0");

    for request in [
        Request::post("/api/packages")
            .header("content-type", "application/json")
            .header("remote-user", "owner")
            .body(Body::from(r#"{"name":"Public.Demo","isPublic":true,"submitForReview":false}"#))
            .unwrap(),
        Request::post("/api/packages/Public.Demo/versions/1.0.0/artifact")
            .header("remote-user", "owner")
            .body(Body::from(artifact))
            .unwrap(),
    ] {
        assert!(app.clone().oneshot(request).await.unwrap().status().is_success());
    }

    let docs = app
        .clone()
        .oneshot(Request::get("/api/packages/Public.Demo/versions/1.0.0/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(docs.status(), StatusCode::OK);
    assert_eq!(json(docs).await[0]["path"], "README.md");

    let doc_file = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Public.Demo/versions/1.0.0/docs/file?path=docs%2Fguide.md")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(doc_file.status(), StatusCode::OK);
    assert_eq!(doc_file.into_body().collect().await.unwrap().to_bytes(), "Use `public.demo`.\n");

    let readme = app
        .clone()
        .oneshot(Request::get("/api/packages/Public.Demo/versions/1.0.0/readme").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(readme.status(), StatusCode::OK);
    assert_eq!(readme.into_body().collect().await.unwrap().to_bytes(), "# Public Demo\n");

    let source = app
        .clone()
        .oneshot(
            Request::get("/api/packages/Public.Demo/versions/1.0.0/source/file?path=src%2Fmain.bsk")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(source.status(), StatusCode::OK);
    assert_eq!(source.into_body().collect().await.unwrap().to_bytes(), "module Main\n");

    let tree = app
        .clone()
        .oneshot(Request::get("/api/packages/Public.Demo/versions/1.0.0/source/tree").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(tree.status(), StatusCode::OK);
    assert_eq!(json(tree).await[0]["path"], "src/main.bsk");

    let metadata = app
        .clone()
        .oneshot(Request::get("/api/packages/Public.Demo/versions/1.0.0/docs/structured").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(metadata.status(), StatusCode::OK);
    assert_eq!(json(metadata).await["metadata"]["title"], "Public Demo");

    let traversal = app
        .oneshot(
            Request::get("/api/packages/Public.Demo/versions/1.0.0/docs/file?path=docs%2F..%2Fpackage.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(traversal.status(), StatusCode::NOT_FOUND);
    std::fs::remove_dir_all(root).expect("artifact root is removed");
}
