//! `type = Template` manifest contract tests (platform-spec project templates).

use std::fs;

use beskid_analysis::projects::{
    PROJECT_FILE_NAME, ProjectKind, build_compile_plan, build_project_graph,
    collect_dependency_projects, parse_manifest,
};

use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

use super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn parses_template_type_and_nested_block() {
    let src = r#"
project {
  name = "beskid-templates-console"
  version = "0.0.0"
  type = Template
  template {
    shortName = "console"
    identity  = "beskid.templates.console"
  }
}
"#;
    let m = parse_manifest(src).expect("parse");
    assert_eq!(m.project.kind, ProjectKind::Template);
    assert!(m.targets.is_empty());
    let template = m.project.template_section.as_ref().expect("template section");
    assert_eq!(template.short_name.as_deref(), Some("console"));
    assert_eq!(
        template.identity.as_deref(),
        Some("beskid.templates.console")
    );
}

#[test]
fn template_projects_reject_target_blocks() {
    let src = r#"
project {
  name = "bad"
  version = "0.1.0"
  type = Template
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#;
    let err = parse_manifest(src).expect_err("targets forbidden");
    assert_eq!(err.code(), "E1878");
}

#[test]
fn rejects_nocorelib_key() {
    let src = r#"
project {
  name = "bad"
  version = "0.1.0"
  noCorelib = true
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#;
    let err = parse_manifest(src).expect_err("noCorelib forbidden");
    assert_eq!(err.code(), "E1876");
}

#[test]
fn rejects_use_corelib_false() {
    let src = r#"
project {
  name = "bad"
  version = "0.1.0"
  useCorelib = false
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#;
    let err = parse_manifest(src).expect_err("useCorelib false forbidden");
    assert_eq!(err.code(), "E1876");
}

#[test]
fn graph_excludes_implicit_std_for_template_root() {
    let root = temp_case_dir("template_graph_no_std");
    let template_dir = root.join("Tpl");
    fs::create_dir_all(template_dir.join("Src")).expect("mkdir");

    write_manifest(
        &template_dir,
        r#"
project {
  name = "Tpl"
  version = "0.1.0"
  type = Template
  template {
    shortName = "tpl"
  }
}
"#,
    );

    let manifest_path = template_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let graph = build_project_graph(&manifest_path).expect("graph");
        assert!(
            !graph.has_std_dependency,
            "template authoring roots must not receive implicit corelib in the project graph"
        );
        assert_eq!(graph.root_manifest.project.kind, ProjectKind::Template);
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_rejects_template_root() {
    let root = temp_case_dir("template_compile_plan");
    let template_dir = root.join("Tpl");
    fs::create_dir_all(template_dir.join("Src")).expect("mkdir");

    write_manifest(
        &template_dir,
        r#"
project {
  name = "Tpl"
  version = "0.1.0"
  type = Template
}
"#,
    );

    let manifest_path = template_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let err = build_compile_plan(&manifest_path, None).expect_err("template build");
        assert_eq!(err.code(), "E1877");
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_collect_skips_template_path_dependency_nodes() {
    let root = temp_case_dir("template_graph_collect");
    let app_dir = root.join("App");
    let template_dir = root.join("Tpl");
    fs::create_dir_all(app_dir.join("Src")).expect("mkdir app");
    fs::create_dir_all(template_dir.join("Src")).expect("mkdir tpl");

    write_manifest(
        &app_dir,
        r#"
project {
  name = "App"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "Tpl" {
  source = path
  path = "../Tpl"
}
"#,
    );

    write_manifest(
        &template_dir,
        r#"
project {
  name = "Tpl"
  version = "0.1.0"
  type = Template
}
"#,
    );

    let manifest_path = app_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let graph = build_project_graph(&manifest_path).expect("graph");
        let deps = collect_dependency_projects(&graph);
        assert!(
            deps.iter().all(|dep| dep.project_name != "Tpl"),
            "template path dependencies must be excluded from compile-plan dependency projection: {deps:#?}"
        );
    });

    let _ = fs::remove_dir_all(root);
}
