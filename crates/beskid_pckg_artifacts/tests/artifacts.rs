use std::io::{Cursor, Write};

use beskid_pckg_artifacts::{
    ArtifactBrowser, ArtifactDependency, ArtifactError, ArtifactFileContent, ArtifactRecord, LocalFileArtifactStore,
    MAX_BROWSE_READ_BYTES, PackageArtifactStore, PackageKind, PublishRequest, canonicalize_project_dependencies,
    select_download, validate_package_artifact,
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
fn browser_lists_and_reads_documentation_source_and_api_docs() {
    let artifact = archive_with_files(
        "acme.math",
        "1.2.3",
        &[
            ("README.md", "# Acme Math\n"),
            ("docs/guide.md", "Use `acme.math`.\n"),
            (
                ".beskid/docs/api.json",
                r#"{"schemaVersion":4,"navigationModel":"graph-v1","generator":"t","source":"src/main.bd","items":[]}"#,
            ),
            ("LICENSE", "Apache-2.0\n"),
            ("src/internal/add.bd", "fn add() {}\n"),
        ],
    );
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert_eq!(
        browser.list_docs().unwrap().iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
        vec!["README.md", ".beskid/docs/api.json", "docs/guide.md"]
    );
    assert_eq!(browser.read_doc("docs/guide.md").unwrap(), "Use `acme.math`.\n");
    assert_eq!(
        browser.list_source_tree().unwrap().iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
        vec!["src/internal/add.bd", "src/main.bd"]
    );
    assert_eq!(browser.read_source("src/internal/add.bd").unwrap(), "fn add() {}\n");
    let documentation = browser.documentation().unwrap();
    assert_eq!(documentation.readme.as_deref(), Some("# Acme Math\n"));
    assert_eq!(documentation.api.unwrap()["schemaVersion"], 4);
    assert_eq!(
        browser.list_files().iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
        vec![
            ".beskid/docs/api.json",
            "LICENSE",
            "README.md",
            "acme_math.bproj",
            "checksums.sha256",
            "docs/guide.md",
            "package.json",
            "src/internal/add.bd",
            "src/main.bd",
        ]
    );
    let license = browser.read_file("LICENSE").unwrap();
    assert_eq!(license.size_bytes, 11);
    assert_eq!(license.content, ArtifactFileContent::Text("Apache-2.0\n".into()));
}

#[test]
fn browser_reads_binary_and_oversized_files_without_decoding_them() {
    let oversized = "x".repeat(1024 * 1024 + 1);
    let artifact =
        archive_with_files("acme.math", "1.2.3", &[("assets/big.txt", &oversized), ("assets/blob.bin", "a\0b")]);
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert_eq!(browser.read_file("assets/blob.bin").unwrap().content, ArtifactFileContent::Binary);
    assert!(matches!(
        browser.read_file("assets/big.txt").unwrap().content,
        ArtifactFileContent::TooLarge { limit_bytes } if limit_bytes == MAX_BROWSE_READ_BYTES
    ));
    assert!(matches!(browser.read_file("assets/../package.json"), Err(ArtifactError::ForbiddenBrowsePath)));
    assert!(matches!(browser.read_file("assets/missing.txt"), Err(ArtifactError::EntryNotFound)));
}

#[test]
fn documentation_reads_api_json_larger_than_the_per_file_browse_limit() {
    let padding = "x".repeat(MAX_BROWSE_READ_BYTES as usize);
    let api = format!(r#"{{"schemaVersion":4,"items":[],"padding":"{padding}"}}"#);
    let artifact = archive_with_files("acme.math", "1.2.3", &[(".beskid/docs/api.json", &api)]);
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert_eq!(browser.documentation().unwrap().api.unwrap()["schemaVersion"], 4);
}

#[test]
fn browser_indexes_docs_without_rejecting_safe_unrelated_artifact_entries() {
    let artifact = archive_with_files(
        "acme.math",
        "1.2.3",
        &[
            ("README.md", "# Acme Math\n"),
            ("docs/guide.md", "Use it.\n"),
            ("Project.lock", "lock data"),
            ("package-icon.svg", "<svg></svg>"),
            ("tests/smoke.bd", "fn test() {}"),
            ("grammars/highlight.json", "{}"),
        ],
    );
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert_eq!(
        browser.list_docs().unwrap().iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(),
        vec!["README.md", "docs/guide.md"]
    );
    assert_eq!(browser.read_doc("docs/guide.md").unwrap(), "Use it.\n");
    assert!(matches!(browser.read_doc("Project.lock"), Err(ArtifactError::ForbiddenBrowsePath)));
    assert!(matches!(browser.read_source("tests/smoke.bd"), Err(ArtifactError::ForbiddenBrowsePath)));
}

#[test]
fn browser_rejects_hidden_and_traversal_reads() {
    for hidden in ["docs/.draft.md", "src/.secret.bd", "src/.beskid/private.bd"] {
        let artifact = archive_with_files("acme.math", "1.2.3", &[(hidden, "not public")]);
        let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();
        assert!(
            matches!(
                ArtifactBrowser::from_validated_bytes(&artifact, &validated),
                Err(ArtifactError::UnsafeBrowseEntry(path)) if path == hidden
            ),
            "{hidden} inside the public docs/source trees must make the artifact unbrowseable"
        );
    }

    let unrelated_hidden =
        archive_with_files("acme.math", "1.2.3", &[(".gitignore", "obj/"), ("assets/.cache/blob", "x")]);
    let validated = validate_package_artifact(&unrelated_hidden, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&unrelated_hidden, &validated).unwrap();
    assert!(browser.list_files().iter().all(|entry| !entry.path.starts_with('.') && !entry.path.contains("/.")));
    assert!(matches!(browser.read_file(".gitignore"), Err(ArtifactError::ForbiddenBrowsePath)));
    assert!(matches!(browser.read_file("assets/.cache/blob"), Err(ArtifactError::ForbiddenBrowsePath)));

    let clean = archive_with_files("acme.math", "1.2.3", &[("docs/guide.md", "safe")]);
    let validated = validate_package_artifact(&clean, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&clean, &validated).unwrap();
    assert!(matches!(browser.read_doc("docs/../package.json"), Err(ArtifactError::ForbiddenBrowsePath)));
    assert!(matches!(browser.read_source("docs/guide.md"), Err(ArtifactError::ForbiddenBrowsePath)));
}

#[test]
fn browser_rejects_nested_hidden_segment_inside_beskid_documentation() {
    let artifact = archive_with_files(
        "acme.math",
        "1.2.3",
        &[
            (".beskid/docs/metadata.json", r#"{"title":"Acme Math"}"#),
            (".beskid/docs/private/.beskid/secret.md", "not public"),
        ],
    );
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();

    assert!(matches!(
        ArtifactBrowser::from_validated_bytes(&artifact, &validated),
        Err(ArtifactError::UnsafeBrowseEntry(path)) if path == ".beskid/docs/private/.beskid/secret.md"
    ));
}

#[test]
fn browser_rejects_oversized_text_reads() {
    let oversized = "x".repeat(1024 * 1024 + 1);
    let artifact = archive_with_files("acme.math", "1.2.3", &[("docs/large.md", &oversized)]);
    let validated = validate_package_artifact(&artifact, "acme.math", "1.2.3").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    assert!(matches!(browser.read_doc("docs/large.md"), Err(ArtifactError::EntryTooLarge { .. })));
}

fn library_archive_with_project(project: &str, files: &[(&str, &str)]) -> Vec<u8> {
    let manifest =
        r#"{"schema":"beskid.package.v1","id":"serialization_mod","version":"0.6.0","packageKind":"library"}"#;
    let mut entries = vec![("package.json", manifest.to_string()), ("serialization_mod.bproj", project.to_string())];
    entries.extend(files.iter().map(|(path, contents)| (*path, (*contents).to_string())));
    archive_from_files(entries)
}

const MOD_PROJECT_WITH_SRC_ROOT: &str = "serialization_mod {\n  name = \"serialization_mod\"\n  version = \"0.6.0\"\n  root = \"Src\"\n  type = Mod\n  mod { capabilities = [read_project_sources, query_semantic_snapshot, emit_syntax] }\n}\n";

#[test]
fn validates_mod_package_with_declared_source_root_and_browses_it() {
    let artifact =
        library_archive_with_project(MOD_PROJECT_WITH_SRC_ROOT, &[("Src/Mod.bd", "pub i32 Value() { return 1; }")]);

    let validated = validate_package_artifact(&artifact, "serialization_mod", "0.6.0").unwrap();
    let browser = ArtifactBrowser::from_validated_bytes(&artifact, &validated).unwrap();

    let sources = browser.list_source_tree().unwrap().into_iter().map(|entry| entry.path).collect::<Vec<_>>();
    assert_eq!(sources, vec!["Src/Mod.bd".to_string()]);
    assert_eq!(browser.read_source("Src/Mod.bd").unwrap(), "pub i32 Value() { return 1; }");
}

#[test]
fn rejects_package_whose_sources_are_outside_the_declared_root() {
    let artifact =
        library_archive_with_project(MOD_PROJECT_WITH_SRC_ROOT, &[("src/Mod.bd", "pub i32 Value() { return 1; }")]);

    assert!(matches!(
        validate_package_artifact(&artifact, "serialization_mod", "0.6.0"),
        Err(ArtifactError::InvalidZip(message)) if message.contains("missing source under its project root Src/")
    ));
}

#[test]
fn rejects_library_without_sources_under_the_default_root() {
    let project = "serialization_mod {\n  name = \"serialization_mod\"\n}\n";
    let artifact = library_archive_with_project(project, &[("README.md", "# docs only")]);

    assert!(matches!(
        validate_package_artifact(&artifact, "serialization_mod", "0.6.0"),
        Err(ArtifactError::InvalidZip(message)) if message.contains("missing source under its project root src/")
    ));
}

#[test]
fn rejects_project_source_root_that_escapes_the_package() {
    for root in ["../outside", "/abs", "..", "Src/../..", ".", "", "C:/src", ".hidden"] {
        let project =
            format!("serialization_mod {{\n  name = \"serialization_mod\"\n  root = \"{root}\"\n  type = Mod\n}}\n");
        let artifact = library_archive_with_project(&project, &[("src/Mod.bd", "x"), ("Src/Mod.bd", "x")]);

        assert!(
            matches!(
                validate_package_artifact(&artifact, "serialization_mod", "0.6.0"),
                Err(ArtifactError::InvalidManifest(message)) if message.contains("must be a relative path inside the package")
            ),
            "root {root:?} must be rejected"
        );
    }
}
