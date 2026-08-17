use std::fs;

use beskid_tests_support::{
    assert_same_canonical_path, temp_case_dir, write_project_manifest as write_manifest, write_workspace_manifest,
};
use beskid_analysis::projects::{
    ProjectError, TargetKind, UnresolvedDependencyPolicy, build_compile_plan, build_compile_plan_with_policy,
};

use super::super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn compile_plan_picks_app_target_by_default() {
    let dir = temp_case_dir("default_app_target");
    let source = r#"
project {
  name = "MyApp"
  version = "0.1.0"
}

target "Library" {
  kind = "Lib"
  entry = "Lib.bd"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}
"#;
    let manifest_path = write_manifest(&dir, source);
    fs::create_dir_all(dir.join("Src")).expect("default source root exists for path resolution");

    with_cwd_at_workspace_root(&dir, || {
        let plan = build_compile_plan(&manifest_path, None).expect("plan should build");
        assert_eq!(plan.project_name, "MyApp");
        assert_eq!(plan.target.name, "App");
        assert_eq!(plan.target.kind, TargetKind::App);
        assert_same_canonical_path(&plan.source_root, dir.join("Src"));
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_applies_workspace_registry_override_version() {
    let dir = temp_case_dir("workspace_override_registry");
    let source = r#"
project {
  name = "MyApp"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}

dependency "PkgCore" {
  source = "registry"
  version = "1.2.3"
  registry = "default"
}
"#;
    let manifest_path = write_manifest(&dir, source);
    write_workspace_manifest(
        &dir,
        r#"
workspace {
  name = "Root"
}

member "app" {
  path = "."
}

override "PkgCore" {
  version = "2.0.0"
}

registry "default" {
  url = "https://pckg.beskid-lang.org"
}
"#,
    );

    with_cwd_at_workspace_root(&dir, || {
        let plan = build_compile_plan_with_policy(&manifest_path, None, UnresolvedDependencyPolicy::Warn)
            .expect("warn policy should collect unresolved deps");
        assert_eq!(plan.unresolved_dependencies.len(), 1);
        assert_eq!(plan.unresolved_dependencies[0].dependency_name, "PkgCore");
        assert_eq!(plan.unresolved_dependencies[0].descriptor, "default@2.0.0");
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_rejects_unknown_workspace_registry_alias() {
    let dir = temp_case_dir("workspace_unknown_registry_alias");
    let source = r#"
project {
  name = "MyApp"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}

dependency "PkgCore" {
  source = "registry"
  version = "1.2.3"
  registry = "private"
}
"#;
    let manifest_path = write_manifest(&dir, source);
    write_workspace_manifest(
        &dir,
        r#"
workspace {
  name = "Root"
}

member "app" {
  path = "."
}

registry "default" {
  url = "https://pckg.beskid-lang.org"
}
"#,
    );

    let error = with_cwd_at_workspace_root(&dir, || {
        build_compile_plan_with_policy(&manifest_path, None, UnresolvedDependencyPolicy::Warn)
            .expect_err("unknown alias should fail validation")
    });
    let message = error.to_string();
    assert!(message.contains("unknown workspace registry alias"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_uses_explicit_target_name() {
    let dir = temp_case_dir("explicit_target");
    let source = r#"
project {
  name = "MyApp"
  version = "0.1.0"
}

target "Library" {
  kind = "Lib"
  entry = "Lib.bd"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}
"#;
    let manifest_path = write_manifest(&dir, source);

    with_cwd_at_workspace_root(&dir, || {
        let plan = build_compile_plan(&manifest_path, Some("Library")).expect("plan should build");
        assert_eq!(plan.target.name, "Library");
        assert_eq!(plan.target.kind, TargetKind::Lib);
    });
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_errors_on_missing_target() {
    let dir = temp_case_dir("missing_target");
    let source = r#"
project {
  name = "MyApp"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}
"#;
    let manifest_path = write_manifest(&dir, source);

    let error =
        with_cwd_at_workspace_root(&dir, || build_compile_plan(&manifest_path, Some("Tests")).expect_err("must fail"));
    assert!(matches!(error, ProjectError::TargetNotFound(_)));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_resolves_path_dependencies_transitively() {
    let root = temp_case_dir("path_dependencies_transitive");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    let util_dir = root.join("Util");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");
    fs::create_dir_all(&util_dir).expect("create util dir");

    let util_manifest = r#"
project {
  name = "Util"
  version = "0.1.0"
}

target "UtilLib" {
  kind = "Lib"
  entry = "Util.bd"
}
"#;
    write_manifest(&util_dir, util_manifest);

    let core_manifest = r#"
project {
  name = "Core"
  version = "0.1.0"
}

target "CoreLib" {
  kind = "Lib"
  entry = "Core.bd"
}

dependency "Util" {
  source = "path"
  path = "../Util"
}
"#;
    write_manifest(&core_dir, core_manifest);

    let app_manifest = r#"
project {
  name = "App"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}

dependency "Core" {
  source = "path"
  path = "../Core"
}
"#;
    let app_manifest_path = write_manifest(&app_dir, app_manifest);

    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        assert!(plan.dependency_projects.len() >= 2);
        assert!(plan.dependency_projects.iter().any(|dependency| dependency.dependency_name == "Util"));
        assert!(plan.dependency_projects.iter().any(|dependency| dependency.dependency_name == "Core"));
    });

    let _ = fs::remove_dir_all(root);
}
