use std::io::{Cursor, Write};

use beskid_pckg_artifacts::{
    ArtifactError, ArtifactRecord, LocalFileArtifactStore, PackageArtifactStore, PublishRequest,
    select_download, validate_package_artifact,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

fn valid_archive(package: &str, version: &str) -> Vec<u8> {
    let manifest =
        format!(r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}"}}"#);
    let project = format!("name = \"{package}\"\n");
    let files = [
        ("package.json", manifest),
        ("Project.proj", project),
        ("src/main.bsk", "fn main() {}".into()),
    ];
    let checksums = files
        .iter()
        .map(|(path, contents)| format!("{:x}  {path}", Sha256::digest(contents.as_bytes())))
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut output);
    for (path, contents) in files.into_iter().chain([("checksums.sha256", checksums)]) {
        zip.start_file(path, SimpleFileOptions::default()).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    output.into_inner()
}

fn archive_with_zip_slip() -> Vec<u8> {
    let files = [
        (
            "package.json",
            r#"{"schema":"beskid.package.v1","id":"acme.math","version":"1.2.3"}"#,
        ),
        ("Project.proj", "name = \"acme.math\"\n"),
        ("src/main.bsk", "fn main() {}"),
        ("../escape.bsk", "bad"),
    ];
    let checksums = files
        .iter()
        .map(|(path, contents)| format!("{:x}  {path}", Sha256::digest(contents.as_bytes())))
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut output);
    for (path, contents) in files
        .into_iter()
        .chain([("checksums.sha256", checksums.as_str())])
    {
        zip.start_file(path, SimpleFileOptions::default()).unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    output.into_inner()
}

#[test]
fn validates_zip_manifest_and_embedded_checksums() {
    let artifact = valid_archive("acme.math", "1.2.3");

    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    assert_eq!(validated.package_name, "acme.math");
    assert_eq!(validated.version, "1.2.3");
    assert_eq!(validated.size_bytes, artifact.len() as u64);
    assert_eq!(validated.checksum_sha256.len(), 64);
}

#[test]
fn rejects_zip_slip_entry_before_storing_artifact() {
    let artifact = archive_with_zip_slip();

    assert!(matches!(
        validate_package_artifact(&artifact, "acme.math", "1.2.3"),
        Err(ArtifactError::InvalidZip(_))
    ));
}

#[test]
fn local_store_round_trips_and_rejects_path_traversal_keys() {
    let temp = tempfile::tempdir().unwrap();
    let store = LocalFileArtifactStore::new(temp.path()).unwrap();
    let artifact = valid_archive("acme.math", "1.2.3");
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    let saved = store
        .save(PublishRequest {
            validated,
            bytes: &artifact,
        })
        .unwrap();
    assert_eq!(store.open(&saved.storage_key).unwrap(), artifact);
    assert!(
        store
            .verify(&saved.storage_key, &saved.checksum_sha256)
            .unwrap()
    );
    assert!(matches!(
        store.open("../../etc/passwd"),
        Err(ArtifactError::InvalidStorageKey)
    ));
}

#[test]
fn download_selection_uses_highest_non_yanked_semver() {
    let versions = vec![
        ArtifactRecord::new("1.9.0", true),
        ArtifactRecord::new("1.10.0", false),
        ArtifactRecord::new("2.0.0-beta.1", false),
        ArtifactRecord::new("broken", false),
    ];

    assert_eq!(
        select_download(&versions, "latest").unwrap().version,
        "2.0.0-beta.1"
    );
    assert_eq!(select_download(&versions, "1.9.0"), None);
    assert_eq!(
        select_download(&versions, "1.10.0").unwrap().version,
        "1.10.0"
    );
}
