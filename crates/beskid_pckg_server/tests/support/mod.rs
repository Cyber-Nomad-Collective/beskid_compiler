use std::io::Write;

use axum::body::Body;
use axum::http::Request;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

#[allow(dead_code)]
pub fn isolated_artifact_root(suite: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("beskid-pckg-{suite}-{}", uuid::Uuid::new_v4()))
}

pub fn artifact(name: &str, version: &str) -> Vec<u8> {
    let manifest =
        format!(r#"{{"schema":"beskid.package.v1","id":"{name}","version":"{version}","packageKind":"library"}}"#,);
    let project_name = name.replace('.', "_").to_ascii_lowercase();
    let project_manifest = format!("{project_name}.bproj");
    let project = format!("{project_name} {{\n  name = \"{project_name}\"\n}}\n");
    let source = "module Main\n";
    archive(vec![
        ("package.json".into(), manifest.into_bytes()),
        (project_manifest, project.into_bytes()),
        ("src/main.bd".into(), source.as_bytes().to_vec()),
    ])
}

#[allow(dead_code)]
pub fn template_artifact(name: &str, version: &str) -> Vec<u8> {
    let project_name = name.replace('.', "_").to_ascii_lowercase();
    let manifest = serde_json::json!({
        "schema": "beskid.package.v1",
        "id": name,
        "version": version,
        "packageKind": "template",
        "template": {
            "identity": format!("{name}::1.0.0"),
            "shortName": "demo",
            "tags": {"type": "project", "classifications": ["starter"]}
        },
        "dependencies": [{"name": "corelib_foundation", "version": "0.4.0", "source": "registry"}]
    })
    .to_string();
    let project = format!(
        "{project_name} {{\n  name = \"{project_name}\"\n  type = Template\n  identity = \"{name}\"\n}}\n\ndependency \"corelib_foundation\" {{\n  source = registry\n  version = \"0.4.0\"\n}}\n",
    );
    let template = serde_json::json!({
        "schema": "beskid.template.v1",
        "identity": format!("{name}::1.0.0"),
        "shortName": "demo",
        "tags": {"type": "project", "classifications": ["starter"]}
    })
    .to_string();
    archive(vec![
        ("package.json".into(), manifest.into_bytes()),
        (format!("{project_name}.bproj"), project.into_bytes()),
        ("template.json".into(), template.into_bytes()),
        ("content/Main.bd".into(), b"fn Main() {}".to_vec()),
    ])
}

#[allow(dead_code)]
pub fn tool_artifact_with_conflicting_template(name: &str, version: &str) -> Vec<u8> {
    let manifest = serde_json::json!({
        "schema": "beskid.package.v1",
        "id": name,
        "version": version,
        "packageKind": "tool",
        "dependencies": []
    })
    .to_string();
    archive(vec![
        ("package.json".into(), manifest.into_bytes()),
        ("template.json".into(), br#"{"schema":"beskid.template.v1"}"#.to_vec()),
    ])
}

fn archive(entries: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let checksums =
        entries.iter().map(|(path, bytes)| format!("{}  {path}", hex_sha256(bytes))).collect::<Vec<_>>().join("\n");
    let mut output = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut output);
    let options = SimpleFileOptions::default();
    for (path, contents) in entries.into_iter().chain([("checksums.sha256".into(), checksums.into_bytes())]) {
        zip.start_file(path, options).expect("entry starts");
        zip.write_all(&contents).expect("entry writes");
    }
    zip.finish().expect("zip finishes");
    output.into_inner()
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn multipart_publish_request(name: &str, version: &str, subject: &str, artifact: Vec<u8>) -> Request<Body> {
    const BOUNDARY: &str = "beskid-pckg-test-boundary";
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
        .header("remote-user", subject)
        .body(Body::from(body))
        .expect("multipart publish request builds")
}
