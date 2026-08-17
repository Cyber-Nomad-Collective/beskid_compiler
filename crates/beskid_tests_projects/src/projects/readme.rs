//! Manifest readme fields and pack artifact layout.

use std::fs;

use beskid_analysis::projects::{
    DEFAULT_README_FILE, PACKAGE_README_ARTIFACT_NAME, ProjectError, discover_readme_for_package_root,
    parse_manifest, parse_workspace_manifest, resolve_readme_from_project_manifest,
    resolve_readme_relative_path,
};
use beskid_pckg::pack::collect_pack_entries;

use beskid_tests_support::{temp_case_dir, write_project_manifest, write_workspace_manifest};

fn minimal_project_manifest(extra_project_fields: &str) -> String {
    format!(
        r#"
project {{
  name = "pkg"
  version = "0.1.0"
{extra_project_fields}
}}

target "Lib" {{
  kind = Lib
  entry = "Main.bd"
}}
"#
    )
}

#[test]
fn parses_optional_project_readme_field() {
    let source = minimal_project_manifest(r#"  readme = "docs/guide.md""#);
    let manifest = parse_manifest(&source).expect("parse");
    assert_eq!(
        manifest.project.readme.as_deref(),
        Some("docs/guide.md")
    );
}

#[test]
fn parses_optional_workspace_readme_field() {
    let source = r#"
workspace {
  name = "Root"
  readme = "readme.md"
}

member "core" {
  path = "corelib"
}
"#;
    let manifest = parse_workspace_manifest(source).expect("parse");
    assert_eq!(
        manifest.workspace.readme.as_deref(),
        Some("readme.md")
    );
}

#[test]
fn rejects_project_readme_path_escape() {
    let source = minimal_project_manifest(r#"  readme = "../secret.md""#);
    let error = parse_manifest(&source).expect_err("must fail");
    assert!(matches!(error, ProjectError::Validation(_)));
}

#[test]
fn defaults_to_readme_md_when_present_on_disk() {
    let dir = temp_case_dir("readme_default_file");
    fs::write(dir.join(DEFAULT_README_FILE), "# Default").expect("write readme");
    write_project_manifest(&dir, &minimal_project_manifest(""));

    let resolved = discover_readme_for_package_root(&dir).expect("discover");
    assert_eq!(resolved.as_deref(), Some(DEFAULT_README_FILE));
}

#[test]
fn explicit_readme_overrides_default_file() {
    let dir = temp_case_dir("readme_explicit_override");
    fs::write(dir.join(DEFAULT_README_FILE), "# Default").expect("write readme");
    fs::write(dir.join("custom.md"), "# Custom").expect("write custom");
    write_project_manifest(
        &dir,
        &minimal_project_manifest(r#"  readme = "custom.md""#),
    );

    let manifest =
        parse_manifest(&std::fs::read_to_string(dir.join("Project.proj")).unwrap()).unwrap();
    let resolved = resolve_readme_from_project_manifest(&dir, &manifest);
    assert_eq!(resolved.as_deref(), Some("custom.md"));
}

#[test]
fn pack_includes_default_readme_md() {
    let dir = temp_case_dir("pack_default_readme");
    fs::write(dir.join(DEFAULT_README_FILE), "# Pack me").expect("write readme");
    write_project_manifest(&dir, &minimal_project_manifest(""));

    let entries = collect_pack_entries(&dir).expect("pack entries");
    let readme = entries
        .iter()
        .find(|(name, _)| name == DEFAULT_README_FILE)
        .map(|(_, bytes)| bytes.as_slice());
    assert_eq!(readme, Some(b"# Pack me".as_slice()));
}

#[test]
fn pack_injects_root_readme_md_for_custom_path() {
    let dir = temp_case_dir("pack_custom_readme");
    fs::create_dir_all(dir.join("docs")).expect("mkdir docs");
    fs::write(dir.join("docs/overview.md"), "# Overview").expect("write overview");
    write_project_manifest(
        &dir,
        &minimal_project_manifest(r#"  readme = "docs/overview.md""#),
    );

    let entries = collect_pack_entries(&dir).expect("pack entries");
    let root_readme = entries
        .iter()
        .find(|(name, _)| name == PACKAGE_README_ARTIFACT_NAME)
        .map(|(_, bytes)| bytes.as_slice());
    assert_eq!(root_readme, Some(b"# Overview".as_slice()));
    assert!(
        entries
            .iter()
            .any(|(name, _)| name == "docs/overview.md"),
        "expected original readme path in artifact"
    );
}

#[test]
fn pack_errors_when_explicit_readme_missing() {
    let dir = temp_case_dir("pack_missing_readme");
    write_project_manifest(
        &dir,
        &minimal_project_manifest(r#"  readme = "missing.md""#),
    );

    let error = collect_pack_entries(&dir).expect_err("must fail");
    let message = format!("{error:?}");
    assert!(message.contains("missing.md"));
}

#[test]
fn workspace_readme_used_when_packing_workspace_root() {
    let dir = temp_case_dir("pack_workspace_readme");
    fs::write(dir.join(DEFAULT_README_FILE), "# Workspace").expect("write readme");
    write_workspace_manifest(
        &dir,
        r#"
workspace {
  name = "Root"
}

member "core" {
  path = "pkg"
}
"#,
    );

    let resolved = discover_readme_for_package_root(&dir).expect("discover");
    assert_eq!(resolved.as_deref(), Some(DEFAULT_README_FILE));

    let entries = collect_pack_entries(&dir).expect("pack entries");
    assert!(
        entries
            .iter()
            .any(|(name, bytes)| name == DEFAULT_README_FILE && bytes == b"# Workspace")
    );
}

#[test]
fn resolve_readme_relative_path_without_manifest_fields() {
    let dir = temp_case_dir("resolve_relative_only");
    assert_eq!(resolve_readme_relative_path(None, &dir), None);
    fs::write(dir.join(DEFAULT_README_FILE), "x").expect("write");
    assert_eq!(
        resolve_readme_relative_path(None, &dir).as_deref(),
        Some(DEFAULT_README_FILE)
    );
}
