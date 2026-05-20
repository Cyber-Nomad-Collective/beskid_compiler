//! **Mod** `Project.proj` / `project.mod` contract tests (platform-spec tooling manifests).

use std::fs;

use beskid_analysis::projects::{
    MOD_CAPABILITY_NAMES, PROJECT_FILE_NAME, ProjectGraphNode, ProjectKind, TargetKind,
    build_compile_plan, build_project_graph, parse_manifest,
};

use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

use super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn parses_mod_nested_block() {
    let src = r#"
project {
  name = "serialization-mod"
  version = "0.1.0"
  type = Mod
  mod {
    maxGeneratorRounds = 6
    capabilities = [read_project_sources, emit_syntax]
    artifactPolicy = reuse
  }
}
"#;
    let m = parse_manifest(src).expect("parse");
    assert_eq!(m.project.kind, ProjectKind::Mod);
    assert!(m.targets.is_empty());
    let mod_section = m.project.mod_section.as_ref().expect("mod section");
    assert_eq!(mod_section.max_generator_rounds, Some(6));
    assert_eq!(
        mod_section.capabilities,
        Some(vec![
            "read_project_sources".to_string(),
            "emit_syntax".to_string()
        ])
    );
    assert_eq!(mod_section.artifact_policy.as_deref(), Some("reuse"));
    assert_eq!(mod_section.resolved_max_generator_rounds(), 6);
}

#[test]
fn legacy_meta_type_and_block_map_to_mod() {
    let src = r#"
project {
  name = "legacy"
  version = "0.1.0"
  type = Meta
  meta {
    maxMetaRounds = 4
    capabilities = [extern_ffi]
    attachTo = default
    entryModules = ["Legacy/Mod.bd"]
  }
}
"#;
    let m = parse_manifest(src).expect("legacy manifest should parse");
    assert_eq!(m.project.kind, ProjectKind::Mod);
    let mod_section = m.project.mod_section.as_ref().expect("mod from meta block");
    assert_eq!(mod_section.max_generator_rounds, Some(4));
    assert_eq!(
        mod_section.capabilities,
        Some(vec!["extern_ffi".to_string()])
    );
}

#[test]
fn mod_projects_reject_target_blocks() {
    let src = r#"
project {
  name = "bad"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax]
  }
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#;
    let err = parse_manifest(src).expect_err("targets forbidden");
    assert_eq!(err.code(), "E1801");
}

#[test]
fn host_projects_reject_mod_block() {
    let src = r#"
project {
  name = "bad"
  version = "0.1.0"
  mod {
    capabilities = [emit_syntax]
  }
}

target "main" {
  kind = App
  entry = "Main.bd"
}
"#;
    let err = parse_manifest(src).expect_err("mod block on host");
    assert_eq!(err.code(), "E1874");
}

#[test]
fn rejects_unknown_mod_capability() {
    let src = r#"
project {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [not_a_real_capability]
  }
}
"#;
    let err = parse_manifest(src).expect_err("unknown capability");
    assert_eq!(err.code(), "E1804");
    assert!(MOD_CAPABILITY_NAMES.contains(&"emit_syntax"));
}

#[test]
fn graph_discovers_transitive_mod_dependency() {
    let root = temp_case_dir("mod_graph_transitive");
    let app_dir = root.join("App");
    let mod_dir = root.join("MyMod");
    fs::create_dir_all(app_dir.join("Src")).expect("mkdir app src");
    fs::create_dir_all(mod_dir.join("Src")).expect("mkdir mod src");
    fs::write(app_dir.join("Src/Main.bd"), "// host\n").expect("main");

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

dependency "MyMod" {
  source = path
  path = "../MyMod"
}
"#,
    );

    write_manifest(
        &mod_dir,
        r#"
project {
  name = "MyMod"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax]
  }
}
"#,
    );

    let manifest_path = app_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let graph = build_project_graph(&manifest_path).expect("graph");
        let mut mod_nodes = 0usize;
        for node in graph.dag.graph().node_weights() {
            if let ProjectGraphNode::ResolvedPathDependency {
                project_name,
                project_kind,
                ..
            } = node
                && project_name == "MyMod"
            {
                assert_eq!(*project_kind, ProjectKind::Mod);
                mod_nodes += 1;
            }
        }
        assert_eq!(mod_nodes, 1, "expected MyMod dependency node");
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_builds_for_mod_root() {
    let root = temp_case_dir("mod_compile_plan");
    let mod_dir = root.join("MyMod");
    fs::create_dir_all(mod_dir.join("Src")).expect("mkdir");

    write_manifest(
        &mod_dir,
        r#"
project {
  name = "MyMod"
  version = "0.1.0"
  type = Mod
  mod {
    maxGeneratorRounds = 3
  }
}
"#,
    );

    let manifest_path = mod_dir.join(PROJECT_FILE_NAME);
    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&manifest_path, None).expect("mod compile plan");
        assert_eq!(plan.project_name, "MyMod");
        assert_eq!(plan.target.name, "__mod__");
        assert_eq!(plan.target.kind, TargetKind::Lib);
    });

    let _ = fs::remove_dir_all(root);
}
