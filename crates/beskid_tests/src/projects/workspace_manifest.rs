use beskid_analysis::projects::{ProjectError, parse_workspace_manifest};

fn base_workspace_manifest() -> &'static str {
    r#"
workspace {
  name = "Root"
}

member "core" {
  path = "corelib"
}
"#
}

#[test]
fn parses_minimal_workspace_manifest() {
    let manifest = parse_workspace_manifest(base_workspace_manifest()).expect("valid workspace manifest");

    assert_eq!(manifest.workspace.name, "Root");
    assert_eq!(manifest.workspace.resolver, "v1");
    assert_eq!(manifest.members.len(), 1);
    assert_eq!(manifest.members[0].name, "core");
    assert_eq!(manifest.members[0].path, "corelib");
}

#[test]
fn parses_workspace_manifest_with_overrides_and_registries() {
    let source = r#"
workspace {
  name = "Root"
  resolver = "v1"
}

member "compiler" {
  path = "compiler"
}

override "Std" {
  version = "1.2.0"
}

registry "default" {
  url = "https://pckg.beskid-lang.org"
}
"#;

    let manifest = parse_workspace_manifest(source).expect("valid workspace manifest");

    assert_eq!(manifest.overrides.len(), 1);
    assert_eq!(manifest.overrides[0].dependency, "Std");
    assert_eq!(manifest.registries.len(), 1);
    assert_eq!(manifest.registries[0].name, "default");
}

#[test]
fn rejects_missing_workspace_block() {
    let source = r#"
member "core" {
  path = "corelib"
}
"#;

    let error = parse_workspace_manifest(source).expect_err("must fail");
    assert!(matches!(error, ProjectError::Validation(_)));
}

#[test]
fn rejects_member_path_escape() {
    let source = r#"
workspace {
  name = "Root"
}

member "core" {
  path = "../corelib"
}
"#;

    let error = parse_workspace_manifest(source).expect_err("must fail");
    assert!(matches!(error, ProjectError::Validation(_)));
}

#[test]
fn rejects_duplicate_member_label() {
    let source = r#"
workspace {
  name = "Root"
}

member "core" {
  path = "corelib"
}

member "core" {
  path = "compiler"
}
"#;

    let error = parse_workspace_manifest(source).expect_err("must fail");
    assert!(matches!(error, ProjectError::Validation(_)));
}
