//! **Mod** `Project.proj` / `project.mod` contract tests (platform-spec tooling manifests).

use std::fs;

use beskid_analysis::projects::{
    MOD_CAPABILITY_NAMES, ProjectGraphNode, ProjectKind, TargetKind, build_compile_plan, build_project_graph,
    parse_manifest,
};

use beskid_tests_support::{temp_case_dir, write_project_manifest as write_manifest};

use super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn parses_mod_nested_block() {
    let src = r#"
serialization_mod {
  name = "serialization_mod"
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
    assert_eq!(mod_section.capabilities, Some(vec!["read_project_sources".to_string(), "emit_syntax".to_string()]));
    assert_eq!(mod_section.artifact_policy.as_deref(), Some("reuse"));
    assert_eq!(mod_section.resolved_max_generator_rounds(), 6);
}

#[test]
fn mod_projects_reject_target_blocks() {
    let src = r#"
bad {
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
bad {
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
m {
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

dependency "MyMod" {
  source = path
  path = "../MyMod"
}
"#,
    );

    write_manifest(
        &mod_dir,
        r#"
MyMod {
  name = "MyMod"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax]
  }
}
"#,
    );
    with_cwd_at_workspace_root(&root, || {
        let graph = build_project_graph(&manifest_path).expect("graph");
        let mut mod_nodes = 0usize;
        for node in graph.dag.graph().node_weights() {
            if let ProjectGraphNode::ResolvedPathDependency { project_name, project_kind, .. } = node
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

    let manifest_path = write_manifest(
        &mod_dir,
        r#"
MyMod {
  name = "MyMod"
  version = "0.1.0"
  type = Mod
  mod {
    maxGeneratorRounds = 3
  }
}
"#,
    );
    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&manifest_path, None).expect("mod compile plan");
        assert_eq!(plan.project_name, "MyMod");
        assert_eq!(plan.target.name, "__mod__");
        assert_eq!(plan.target.kind, TargetKind::Lib);
    });

    let _ = fs::remove_dir_all(root);
}

// --- Conformance evidence: manifest goldens — edge cases (E1803, E1805, E1872, E1873, E1875) ---

#[test]
fn rejects_max_generator_rounds_zero_with_e1803() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    maxGeneratorRounds = 0
  }
}
"#;
    let err = parse_manifest(src).expect_err("maxGeneratorRounds = 0");
    assert_eq!(err.code(), "E1803");
}

#[test]
fn accepts_max_generator_rounds_one() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    maxGeneratorRounds = 1
  }
}
"#;
    let m = parse_manifest(src).expect("maxGeneratorRounds = 1 is valid");
    let mod_section = m.project.mod_section.expect("mod section");
    assert_eq!(mod_section.resolved_max_generator_rounds(), 1);
}

#[test]
fn rejects_unknown_artifact_policy_with_e1805() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    artifactPolicy = "never"
  }
}
"#;
    let err = parse_manifest(src).expect_err("unknown artifactPolicy should fail");
    assert_eq!(err.code(), "E3003");
}

#[test]
fn parses_artifact_policy_rebuild() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    artifactPolicy = "rebuild"
  }
}
"#;
    let m = parse_manifest(src).expect("artifactPolicy = rebuild");
    let mod_section = m.project.mod_section.expect("mod section");
    assert_eq!(mod_section.artifact_policy.as_deref(), Some("rebuild"));
}

#[test]
fn parses_artifact_policy_clean_rebuild() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    artifactPolicy = "clean_rebuild"
  }
}
"#;
    let m = parse_manifest(src).expect("artifactPolicy = clean_rebuild");
    let mod_section = m.project.mod_section.expect("mod section");
    assert_eq!(mod_section.artifact_policy.as_deref(), Some("clean_rebuild"));
}

#[test]
fn parses_artifact_policy_reuse_default() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    artifactPolicy = "reuse"
  }
}
"#;
    let m = parse_manifest(src).expect("artifactPolicy = reuse");
    let mod_section = m.project.mod_section.expect("mod section");
    assert_eq!(mod_section.artifact_policy.as_deref(), Some("reuse"));
}

#[test]
fn rejects_non_numeric_max_generator_rounds_at_parse_level() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    maxGeneratorRounds = "three"
  }
}
"#;
    let err = parse_manifest(src).expect_err("string maxGeneratorRounds should fail");
    assert_eq!(err.code(), "E3003");
}

#[test]
fn rejects_artifact_policy_as_list_at_parse_level() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    artifactPolicy = [reuse, rebuild]
  }
}
"#;
    let err = parse_manifest(src).expect_err("list artifactPolicy should fail");
    assert_eq!(err.code(), "E3003");
}

#[test]
fn parses_multiple_capabilities() {
    let src = r#"
m {
  name = "m"
  version = "0.1.0"
  type = Mod
  mod {
    capabilities = [emit_syntax, read_project_sources, query_semantic_snapshot]
  }
}
"#;
    let m = parse_manifest(src).expect("multiple capabilities");
    let mod_section = m.project.mod_section.expect("mod section");
    let caps = mod_section.capabilities.expect("capabilities");
    assert_eq!(caps.len(), 3);
    assert!(MOD_CAPABILITY_NAMES.contains(&"emit_syntax"));
}
