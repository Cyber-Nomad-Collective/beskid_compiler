use std::io::{Cursor, Write};

use beskid_pckg_artifacts::{
    ArtifactBrowser, ArtifactError, ArtifactRecord, LocalFileArtifactStore, PackageArtifactStore, PublishRequest,
    select_download, validate_package_artifact,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

fn valid_archive(package: &str, version: &str) -> Vec<u8> {
    let manifest = format!(r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}"}}"#);
    let project = format!("name = \"{package}\"\n");
    let files = [("package.json", manifest), ("Project.proj", project), ("src/main.bsk", "fn main() {}".into())];
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

fn archive_with_files(package: &str, version: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let manifest = format!(r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}"}}"#);
    let project = format!("name = \"{package}\"\n");
    let mut files =
        vec![("package.json", manifest), ("Project.proj", project), ("src/main.bsk", "fn main() {}".into())];
    files.extend(extra.iter().map(|(path, contents)| (*path, (*contents).into())));
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
        ("package.json", r#"{"schema":"beskid.package.v1","id":"acme.math","version":"1.2.3"}"#),
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
    for (path, contents) in files.into_iter().chain([("checksums.sha256", checksums.as_str())]) {
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

    assert!(matches!(validate_package_artifact(&artifact, "acme.math", "1.2.3"), Err(ArtifactError::InvalidZip(_))));
}

#[test]
fn local_store_round_trips_and_rejects_path_traversal_keys() {
    let temp = tempfile::tempdir().unwrap();
    let store = LocalFileArtifactStore::new(temp.path()).unwrap();
    let artifact = valid_archive("acme.math", "1.2.3");
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    let saved = store.save(PublishRequest { validated, bytes: &artifact }).unwrap();
    assert_eq!(store.open(&saved.storage_key).unwrap(), artifact);
    assert!(store.verify(&saved.storage_key, &saved.checksum_sha256).unwrap());
    assert!(matches!(store.open("../../etc/passwd"), Err(ArtifactError::InvalidStorageKey)));
}

#[test]
fn download_selection_uses_highest_non_yanked_semver() {
    let versions = vec![
        ArtifactRecord::new("1.9.0", true),
        ArtifactRecord::new("1.10.0", false),
        ArtifactRecord::new("2.0.0-beta.1", false),
        ArtifactRecord::new("broken", false),
    ];

    assert_eq!(select_download(&versions, "latest").unwrap().version, "2.0.0-beta.1");
    assert_eq!(select_download(&versions, "1.9.0"), None);
    assert_eq!(select_download(&versions, "1.10.0").unwrap().version, "1.10.0");
}

#[test]
fn browser_lists_and_reads_documentation_source_and_metadata() {
    let artifact = archive_with_files(
        "acme.math",
        "1.2.3",
        &[
            ("README.md", "# Acme Math\n"),
            ("docs/guide.md", "Use `acme.math`.\n"),
            (".beskid/docs/metadata.json", r#"{"title":"Acme Math","order":1}"#),
            ("src/internal/add.bsk", "fn add() {}\n"),
        ],
    );
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert_eq!(
        browser.list_docs().unwrap().iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
        vec!["README.md", ".beskid/docs/metadata.json", "docs/guide.md"]
    );
    assert_eq!(browser.read_doc("docs/guide.md").unwrap(), "Use `acme.math`.\n");
    assert_eq!(
        browser.list_source_tree().unwrap().iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
        vec!["src/internal/add.bsk", "src/main.bsk"]
    );
    assert_eq!(browser.read_source("src/internal/add.bsk").unwrap(), "fn add() {}\n");
    let documentation = browser.documentation().unwrap();
    assert_eq!(documentation.readme.as_deref(), Some("# Acme Math\n"));
    assert_eq!(documentation.metadata.unwrap()["title"], "Acme Math");
}

#[test]
fn browser_rejects_hidden_and_traversal_reads() {
    let artifact = archive_with_files(
        "acme.math",
        "1.2.3",
        &[("docs/.draft.md", "not public"), ("src/.secret.bsk", "not public")],
    );
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    assert!(matches!(
        ArtifactBrowser::from_validated_bytes(&artifact, &validated),
        Err(ArtifactError::UnsafeBrowseEntry(_))
    ));

    let nested_hidden = archive_with_files("acme.math", "1.2.3", &[("src/.beskid/private.bsk", "not public")]);
    let validated = validate_package_artifact(&nested_hidden, "acme.math", "1.2.3").unwrap();
    assert!(matches!(
        ArtifactBrowser::from_validated_bytes(&nested_hidden, &validated),
        Err(ArtifactError::UnsafeBrowseEntry(_))
    ));

    let clean = archive_with_files("acme.math", "1.2.3", &[("docs/guide.md", "safe")]);
    let validated = validate_package_artifact(&clean, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&clean, &validated).unwrap();
    assert!(matches!(browser.read_doc("docs/../package.json"), Err(ArtifactError::ForbiddenBrowsePath)));
    assert!(matches!(browser.read_source("docs/guide.md"), Err(ArtifactError::ForbiddenBrowsePath)));
}

#[test]
fn browser_rejects_oversized_text_reads() {
    let oversized = "x".repeat(1024 * 1024 + 1);
    let artifact = archive_with_files("acme.math", "1.2.3", &[("docs/large.md", &oversized)]);
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert!(matches!(browser.read_doc("docs/large.md"), Err(ArtifactError::EntryTooLarge { .. })));
}
