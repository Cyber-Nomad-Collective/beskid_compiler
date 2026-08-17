use std::fs;

use beskid_tests_support::{temp_case_dir, write_project_manifest as write_manifest};
use beskid_analysis::projects::{
    DependencySource, ProjectError, UnresolvedDependencyPolicy, build_compile_plan, build_compile_plan_with_policy,
};

use super::super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn compile_plan_errors_when_dependency_manifest_missing() {
    let root = temp_case_dir("missing_dependency_manifest");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create empty core dir");

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

    let error =
        with_cwd_at_workspace_root(&root, || build_compile_plan(&app_manifest_path, None).expect_err("must fail"));
    assert!(matches!(error, ProjectError::DependencyManifestNotFound { .. }));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_errors_on_dependency_cycle() {
    let root = temp_case_dir("dependency_cycle");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");

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

    let core_manifest = r#"
project {
  name = "Core"
  version = "0.1.0"
}

target "CoreLib" {
  kind = "Lib"
  entry = "Core.bd"
}

dependency "App" {
  source = "path"
  path = "../App"
}
"#;
    write_manifest(&core_dir, core_manifest);

    let error =
        with_cwd_at_workspace_root(&root, || build_compile_plan(&app_manifest_path, None).expect_err("must fail"));
    assert!(matches!(error, ProjectError::DependencyCycle(_)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_detects_std_dependency_when_present() {
    let root = temp_case_dir("std_dependency_disables_fallback");
    let app_dir = root.join("App");
    let std_dir = root.join("Std");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&std_dir).expect("create std dir");

    let std_manifest = r#"
project {
  name = "Std"
  version = "0.1.0"
}

target "CoreLib" {
  kind = "Lib"
  entry = "Prelude.bd"
}
"#;
    write_manifest(&std_dir, std_manifest);

    let app_manifest = r#"
project {
  name = "App"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}

dependency "Std" {
  source = "path"
  path = "../Std"
}
"#;
    let app_manifest_path = write_manifest(&app_dir, app_manifest);

    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        assert!(plan.has_std_dependency);
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_injects_std_dependency_when_not_declared() {
    let _guard = super::super::std_dependency_env_lock();
    let root = temp_case_dir("implicit_std_dependency");
    let app_dir = root.join("App");
    let std_dir = root.join("StdBundled");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(std_dir.join("Src")).expect("create std src dir");

    write_manifest(
        &std_dir,
        r#"
project {
  name = "Std"
  version = "1.0.0"
}

target "CoreLib" {
  kind = "Lib"
  entry = "Prelude.bd"
}
"#,
    );
    fs::write(std_dir.join("Src/Prelude.bd"), "unit prelude() { }\n").expect("write std prelude");

    let app_manifest_path = write_manifest(
        &app_dir,
        r#"
project {
  name = "App"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}
"#,
    );

    with_cwd_at_workspace_root(&root, || {
        unsafe { std::env::set_var("BESKID_CORELIB_ROOT", &std_dir) };
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        assert!(plan.has_std_dependency);
        assert!(plan.dependency_projects.iter().any(|dependency| dependency.dependency_name == "Std"));
        unsafe { std::env::remove_var("BESKID_CORELIB_ROOT") };
    });
    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_collects_unresolved_dependencies_in_warn_mode() {
    let dir = temp_case_dir("unresolved_warn_mode");
    let source = r#"
project {
  name = "MyApp"
  version = "0.1.0"
}

target "App" {
  kind = "App"
  entry = "Main.bd"
}

dependency "RemoteStd" {
  source = "git"
  url = "git@example.com/std.git"
  rev = "abc123"
}
"#;
    let manifest_path = write_manifest(&dir, source);

    with_cwd_at_workspace_root(&dir, || {
        let plan = build_compile_plan_with_policy(&manifest_path, None, UnresolvedDependencyPolicy::Warn)
            .expect("warn policy should collect unresolved deps");
        assert_eq!(plan.unresolved_dependencies.len(), 1);
        assert_eq!(plan.unresolved_dependencies[0].dependency_name, "RemoteStd");
        assert_eq!(plan.unresolved_dependencies[0].descriptor, "git@example.com/std.git@abc123");
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_errors_on_unresolved_dependencies_in_strict_mode() {
    let dir = temp_case_dir("unresolved_strict_mode");
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
  source = "git"
  url = "https://example.com/pkg.git"
  rev = "abc123"
}
"#;
    let manifest_path = write_manifest(&dir, source);

    let error = with_cwd_at_workspace_root(&dir, || {
        build_compile_plan_with_policy(&manifest_path, None, UnresolvedDependencyPolicy::Error)
            .expect_err("strict mode must fail")
    });
    assert!(matches!(error, ProjectError::UnresolvedExternalDependencies(_)));
    let message = error.to_string();
    assert!(message.contains("PkgCore"));
    assert!(message.contains("Git"));
    assert!(message.contains("abc123"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_keeps_registry_dependencies_in_strict_mode_for_materialization() {
    let dir = temp_case_dir("registry_allowed_strict_mode");
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
}
"#;
    let manifest_path = write_manifest(&dir, source);

    with_cwd_at_workspace_root(&dir, || {
        let plan = build_compile_plan_with_policy(&manifest_path, None, UnresolvedDependencyPolicy::Error)
            .expect("registry dependencies should be kept for workspace materialization");
        assert_eq!(plan.unresolved_dependencies.len(), 1);
        assert_eq!(plan.unresolved_dependencies[0].dependency_name, "PkgCore");
        assert_eq!(plan.unresolved_dependencies[0].source, DependencySource::Registry);
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn compile_plan_cycle_error_includes_chain_separator() {
    let root = temp_case_dir("cycle_message_chain");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");

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

    let core_manifest = r#"
project {
  name = "Core"
  version = "0.1.0"
}

target "CoreLib" {
  kind = "Lib"
  entry = "Core.bd"
}

dependency "App" {
  source = "path"
  path = "../App"
}
"#;
    write_manifest(&core_dir, core_manifest);

    let error =
        with_cwd_at_workspace_root(&root, || build_compile_plan(&app_manifest_path, None).expect_err("must fail"));
    let message = error.to_string();
    assert!(message.contains(" -> "));

    let _ = fs::remove_dir_all(root);
}
