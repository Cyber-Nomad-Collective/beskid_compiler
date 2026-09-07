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
    let manifest = format!(r#"{{"schema":"beskid.package.v1","id":"{name}","version":"{version}"}}"#);
    let project_name = name.replace('.', "_").to_ascii_lowercase();
    let project_manifest = format!("{project_name}.bproj");
    let project = format!("{project_name} {{\n  name = \"{project_name}\"\n}}\n");
    let source = "module Main\n";
    let checksums = [
        ("package.json", manifest.as_bytes()),
        (project_manifest.as_str(), project.as_bytes()),
        ("src/main.bd", source.as_bytes()),
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
        (project_manifest.as_str(), project.as_bytes()),
        ("src/main.bd", source.as_bytes()),
        ("checksums.sha256", checksums.as_bytes()),
    ] {
        zip.start_file(path, options).expect("entry starts");
        zip.write_all(contents).expect("entry writes");
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
