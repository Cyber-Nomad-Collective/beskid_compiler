use std::io::{Cursor, Write};

use beskid_pckg_artifacts::{
    ArtifactBrowser, ArtifactDependency, ArtifactError, ArtifactRecord, LocalFileArtifactStore, PackageArtifactStore,
    PackageKind, PublishRequest, canonicalize_project_dependencies, select_download, validate_package_artifact,
};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

fn archive_from_files(files: Vec<(&str, String)>) -> Vec<u8> {
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

fn valid_archive(package: &str, version: &str) -> Vec<u8> {
    let manifest =
        format!(r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"library"}}"#,);
    let project = "acme_math {\n  name = \"acme_math\"\n}\n".to_string();
    archive_from_files(vec![
        ("package.json", manifest),
        ("acme_math.bproj", project),
        ("src/main.bd", "fn main() {}".into()),
    ])
}

fn archive_with_files(package: &str, version: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let manifest =
        format!(r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"library"}}"#,);
    let project = "acme_math {\n  name = \"acme_math\"\n}\n".to_string();
    let mut files =
        vec![("package.json", manifest), ("acme_math.bproj", project), ("src/main.bd", "fn main() {}".into())];
    files.extend(extra.iter().map(|(path, contents)| (*path, (*contents).into())));
    archive_from_files(files)
}

fn archive_with_zip_slip() -> Vec<u8> {
    let files = [
        (
            "package.json",
            r#"{"schema":"beskid.package.v1","id":"acme.math","version":"1.2.3","packageKind":"library"}"#,
        ),
        ("acme_math.bproj", "acme_math {\n  name = \"acme_math\"\n}\n"),
        ("src/main.bd", "fn main() {}"),
        ("../escape.bd", "bad"),
    ];
    archive_from_files(files.into_iter().map(|(path, contents)| (path, contents.to_string())).collect())
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
fn rejects_manifest_without_required_package_kind() {
    let artifact = archive_from_files(vec![
        ("package.json", r#"{"schema":"beskid.package.v1","id":"acme.math","version":"1.2.3"}"#.into()),
        ("acme_math.bproj", "acme_math {\n  name = \"acme_math\"\n}\n".into()),
        ("src/main.bd", "fn main() {}".into()),
    ]);

    assert!(matches!(
        validate_package_artifact(&artifact, "acme.math", "1.2.3"),
        Err(ArtifactError::InvalidManifest(message)) if message.contains("packageKind is required")
    ));
}

#[test]
fn validates_canonical_template_artifact_without_legacy_project_or_src_paths() {
    let package = "beskid.templates.console";
    let version = "0.1.1";
    let manifest = format!(
        r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"template","template":{{"identity":"beskid.templates.console::1.0.0","shortName":"console","tags":{{"type":"project"}}}}}}"#,
    );
    let files = [
        ("package.json", manifest),
        (
            "beskid_templates_console.bproj",
            "beskid_templates_console {\n  name = \"beskid_templates_console\"\n  type = Template\n  template {\n    identity = \"beskid.templates.console\"\n  }\n}\n".into(),
        ),
        (
            "template.json",
            r#"{"schema":"beskid.template.v1","identity":"beskid.templates.console::1.0.0","shortName":"console","tags":{"type":"project"}}"#.into(),
        ),
        ("content/Main.bd", "pub i32 Main() { return 0; }".into()),
    ];
    let validated = validate_package_artifact(&archive_from_files(files.into()), package, version).unwrap();

    assert_eq!(validated.metadata.package_kind, PackageKind::Template);
    assert_eq!(validated.metadata.template.as_ref().unwrap()["shortName"], "console");
    assert!(validated.metadata.dependencies.is_empty());
}

#[test]
fn rejects_template_manifest_with_wrong_schema_identity_or_summary() {
    let package = "beskid.templates.console";
    let version = "0.1.1";
    let project = "beskid_templates_console {\n  name = \"beskid_templates_console\"\n  type = Template\n  template {\n    identity = \"beskid.templates.console\"\n  }\n}\n";
    let package_manifest = format!(
        r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"template","template":{{"identity":"beskid.templates.console::1.0.0","shortName":"console"}}}}"#,
    );
    for template in [
        r#"{"schema":"beskid.template.v0","identity":"beskid.templates.console::1.0.0","shortName":"console"}"#,
        r#"{"schema":"beskid.template.v1","identity":"beskid.templates.other::1.0.0","shortName":"console"}"#,
        r#"{"schema":"beskid.template.v1","identity":"beskid.templates.console::1.0.0","shortName":"different"}"#,
    ] {
        let artifact = archive_from_files(vec![
            ("package.json", package_manifest.clone()),
            ("beskid_templates_console.bproj", project.into()),
            ("template.json", template.into()),
            ("content/Main.bd", "pub i32 Main() { return 0; }".into()),
        ]);
        assert!(matches!(
            validate_package_artifact(&artifact, package, version),
            Err(ArtifactError::InvalidManifest(_))
        ));
    }
}

#[test]
fn canonicalizes_planned_path_dependencies_and_rejects_unplanned_paths() {
    let source = r#"corelib_runtime {
  name = "corelib_runtime"
}

dependency "corelib_foundation" {
  source = path
  path = "../foundation"
}
"#;
    let plan = [ArtifactDependency::registry("corelib_foundation", "0.4.2")];

    let (rewritten, dependencies) = canonicalize_project_dependencies(source, &plan).unwrap();

    assert_eq!(dependencies, plan);
    assert!(rewritten.contains("source = registry"));
    assert!(rewritten.contains("version = \"0.4.2\""));
    assert!(!rewritten.contains("path ="));
    assert!(canonicalize_project_dependencies(source, &[]).is_err());
}

#[test]
fn rejects_artifact_whose_project_dependencies_are_path_based_or_disagree_with_package_json() {
    let package = "corelib_runtime";
    let version = "0.4.2";
    let path_project = r#"corelib_runtime {
  name = "corelib_runtime"
}
dependency "corelib_foundation" {
  source = path
  path = "../foundation"
}
"#;
    let registry_project =
        path_project.replace("source = path\n  path = \"../foundation\"", "source = registry\n  version = \"0.4.2\"");
    let package_manifest = format!(
        r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"library","dependencies":[{{"name":"corelib_foundation","version":"0.4.2","source":"registry"}}]}}"#,
    );
    let path_artifact = archive_from_files(vec![
        ("package.json", package_manifest.clone()),
        ("corelib_runtime.bproj", path_project.into()),
        ("src/main.bd", "fn main() {}".into()),
    ]);
    assert!(matches!(
        validate_package_artifact(&path_artifact, package, version),
        Err(ArtifactError::InvalidManifest(message)) if message.contains("registry")
    ));

    let mismatched_manifest = package_manifest.replace("0.4.2\",\"source", "0.4.3\",\"source");
    let mismatched_artifact = archive_from_files(vec![
        ("package.json", mismatched_manifest),
        ("corelib_runtime.bproj", registry_project),
        ("src/main.bd", "fn main() {}".into()),
    ]);
    assert!(matches!(
        validate_package_artifact(&mismatched_artifact, package, version),
        Err(ArtifactError::InvalidManifest(message)) if message.contains("dependencies")
    ));
}

#[test]
fn validates_canonical_aggregate_without_sources() {
    let package = "corelib";
    let version = "0.1.1";
    let artifact = archive_from_files(vec![
        (
            "package.json",
            format!(
                r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"library"}}"#,
            ),
        ),
        ("corelib.bproj", "corelib {\n  name = \"corelib\"\n  type = Aggregate\n}\n".into()),
    ]);

    validate_package_artifact(&artifact, package, version).unwrap();
}

#[test]
fn rejects_legacy_project_manifest() {
    let package = "acme.math";
    let version = "1.2.3";
    let artifact = archive_from_files(vec![
        (
            "package.json",
            format!(
                r#"{{"schema":"beskid.package.v1","id":"{package}","version":"{version}","packageKind":"library"}}"#,
            ),
        ),
        ("Project.proj", "project { name = \"acme_math\" }\n".into()),
        ("src/main.bd", "fn main() {}".into()),
    ]);

    assert!(matches!(
        validate_package_artifact(&artifact, package, version),
        Err(ArtifactError::InvalidZip(message)) if message.contains(".bproj")
    ));
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
            ("src/internal/add.bd", "fn add() {}\n"),
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
        vec!["src/internal/add.bd", "src/main.bd"]
    );
    assert_eq!(browser.read_source("src/internal/add.bd").unwrap(), "fn add() {}\n");
    let documentation = browser.documentation().unwrap();
    assert_eq!(documentation.readme.as_deref(), Some("# Acme Math\n"));
    assert_eq!(documentation.metadata.unwrap()["title"], "Acme Math");
}

#[test]
fn browser_rejects_hidden_and_traversal_reads() {
    let artifact =
        archive_with_files("acme.math", "1.2.3", &[("docs/.draft.md", "not public"), ("src/.secret.bd", "not public")]);
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    assert!(matches!(
        ArtifactBrowser::from_validated_bytes(&artifact, &validated),
        Err(ArtifactError::UnsafeBrowseEntry(_))
    ));

    let nested_hidden = archive_with_files("acme.math", "1.2.3", &[("src/.beskid/private.bd", "not public")]);
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
