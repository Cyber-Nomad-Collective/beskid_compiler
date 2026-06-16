//! `type = Template` manifest contract tests and scaffolding conformance.

use std::fs;

use beskid_analysis::projects::{
    ProjectKind, build_compile_plan, build_project_graph, discover_project_manifest_in_dir,
    collect_dependency_projects, parse_manifest,
};

use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

use super::std_dependency_env_lock;
use super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn parses_template_type_and_nested_block() {
    let src = r#"
beskid_templates_console {
  name = "beskid_templates_console"
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
    let template = m
        .project
        .template_section
        .as_ref()
        .expect("template section");
    assert_eq!(template.short_name.as_deref(), Some("console"));
    assert_eq!(
        template.identity.as_deref(),
        Some("beskid.templates.console")
    );
}

#[test]
fn template_projects_reject_target_blocks() {
    let src = r#"
bad {
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
bad {
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
bad {
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

    let manifest_path = write_manifest(
        &template_dir,
        r#"
Tpl {
  name = "Tpl"
  version = "0.1.0"
  type = Template
  template {
    shortName = "tpl"
  }
}
"#,
    );
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

    let manifest_path = write_manifest(
        &template_dir,
        r#"
Tpl {
  name = "Tpl"
  version = "0.1.0"
  type = Template
}
"#,
    );
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

    let manifest_path = write_manifest(
        &app_dir,
        r#"
App {
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
Tpl {
  name = "Tpl"
  version = "0.1.0"
  type = Template
}
"#,
    );
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

#[test]
fn instantiated_project_analyzes_cleanly() {
    use beskid_analysis::services::{self, FrontEndOptions, PrepareOptions};
    use beskid_template::{
        InstantiateOptions, SymbolCollectOptions, TEMPLATE_MANIFEST_REL, TEMPLATE_SCHEMA,
        instantiate, load_manifest_from_template_root,
    };

    fn write_inline_fixture(root: &std::path::Path) -> std::path::PathBuf {
        let template_dir = root.join("inline-console");
        fs::create_dir_all(template_dir.join("content")).unwrap();
        let manifest = serde_json::json!({
            "schema": TEMPLATE_SCHEMA,
            "identity": "test.inline.console::1.0.0",
            "name": "Inline Console",
            "shortName": "inline-console",
            "tags": { "type": "project" },
            "sourceName": "MyApp",
            "symbols": {
                "name": { "type": "string", "isRequired": true, "defaultValue": "MyApp" }
            },
            "sources": [{ "source": "./content/", "target": "./" }]
        });
        let manifest_path = template_dir.join(TEMPLATE_MANIFEST_REL);
        if let Some(parent) = manifest_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        fs::create_dir_all(template_dir.join("content/Src")).unwrap();
        fs::write(
            template_dir.join("content/MyApp.bproj"),
            r#"{{name}} {
  name    = "{{name}}"
  version = "0.1.0"
  root    = "Src"
}

target "app" {
  kind  = App
  entry = "Main.bd"
}
"#,
        )
        .unwrap();
        fs::write(template_dir.join("content/Src/Main.bd"), "unit Main() {}\n").unwrap();
        template_dir
    }

    let _env = std_dependency_env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let template_root = write_inline_fixture(temp.path());
    let manifest =
        load_manifest_from_template_root(&template_root).expect("load template manifest");
    let output = temp.path().join("scaffolded");
    let options = InstantiateOptions {
        template_root,
        output: output.clone(),
        host_project: None,
        force: false,
        allow_project_manifest: false,
        strict_post_actions: false,
        symbol_options: SymbolCollectOptions {
            interactive: false,
            no_interactive: true,
            primary_name: Some("ScaffoldApp".to_string()),
            bindings: Default::default(),
        },
        skip_default_lock: true,
        beskid_exe: None,
    };
    instantiate(&manifest, &options).expect("instantiate");

    let entry = output.join("Src/Main.bd");
    assert!(
        entry.is_file(),
        "expected entry source at {}",
        entry.display()
    );

    let manifest_path = discover_project_manifest_in_dir(&output)
        .expect("discover scaffold manifest")
        .expect("scaffold manifest present");
    let resolved = services::resolve_input(
        Some(&entry),
        Some(&manifest_path),
        None,
        None,
        false,
        false,
    )
    .expect("resolve instantiated project");

    let (_, diagnostics) = beskid_queries::prepare_compilation_diagnostics(
        &resolved,
        PrepareOptions {
            front_end: FrontEndOptions {
                with_semantic_diagnostics: true,
                ..Default::default()

            },
            ..Default::default()
        },
        None,
    )
    .expect("analyze instantiated project");

    services::require_no_semantic_errors(&diagnostics).expect("no semantic errors");
}
