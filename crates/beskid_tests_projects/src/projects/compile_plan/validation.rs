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

dependency "Kernel" {
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

dependency "Kernel" {
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

fn write_corelib_aggregate(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("Src")).expect("create corelib src dir");
    write_manifest(
        dir,
        r#"
project {
  name = "corelib"
  version = "1.0.0"
}

target "CoreLib" {
  kind = "Lib"
  entry = "Prelude.bd"
}
"#,
    );
    fs::write(dir.join("Src/Prelude.bd"), "unit prelude() { }\n").expect("write corelib prelude");
}

fn write_app_manifest(dir: &std::path::Path, dependencies: &str) -> std::path::PathBuf {
    write_manifest(
        dir,
        &format!(
            r#"
project {{
  name = "App"
  version = "0.1.0"
}}

target "App" {{
  kind = "App"
  entry = "Main.bd"
}}
{dependencies}"#
        ),
    )
}

#[test]
fn compile_plan_detects_core_dependency_when_present() {
    let root = temp_case_dir("core_dependency_disables_fallback");
    let app_dir = root.join("App");
    let corelib_dir = root.join("CorelibCheckout");
    fs::create_dir_all(&app_dir).expect("create app dir");
    write_corelib_aggregate(&corelib_dir);
    let app_manifest_path = write_app_manifest(
        &app_dir,
        "\ndependency \"Core\" {\n  source = \"path\"\n  path = \"../CorelibCheckout\"\n}\n",
    );

    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        assert!(plan.has_core_dependency);
        assert_eq!(
            plan.dependency_projects.iter().filter(|dependency| dependency.dependency_name == "Core").count(),
            1,
            "an explicit `Core` dependency replaces the implicit one"
        );
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_rejects_core_label_for_a_non_corelib_project() {
    let root = temp_case_dir("core_label_reserved");
    let app_dir = root.join("App");
    let shared_dir = root.join("Shared");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(shared_dir.join("Src")).expect("create shared dir");
    write_manifest(
        &shared_dir,
        "project {\n  name = \"Shared\"\n  version = \"0.1.0\"\n}\n\ntarget \"SharedLib\" {\n  kind = \"Lib\"\n  entry = \"Shared.bd\"\n}\n",
    );
    let app_manifest_path = write_app_manifest(
        &app_dir,
        "\ndependency \"Core\" {\n  source = \"path\"\n  path = \"../Shared\"\n}\n",
    );

    let error = with_cwd_at_workspace_root(&root, || {
        build_compile_plan(&app_manifest_path, None).expect_err("`Core` is reserved for the Corelib aggregate")
    });
    assert!(
        matches!(&error, ProjectError::Validation(message) if message.contains("reserved for the Corelib aggregate")),
        "{error}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn compile_plan_injects_core_dependency_when_not_declared() {
    let root = temp_case_dir("implicit_core_dependency");
    let app_dir = root.join("App");
    let corelib_dir = root.join("CorelibBundled");
    fs::create_dir_all(&app_dir).expect("create app dir");
    write_corelib_aggregate(&corelib_dir);
    let app_manifest_path = write_app_manifest(&app_dir, "");

    let _core_root = super::super::scoped_core_dependency_root(&corelib_dir);
    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        assert!(plan.has_core_dependency);
        assert!(plan.dependency_projects.iter().any(|dependency| dependency.dependency_name == "Core"));
        assert!(plan.dependency_projects.iter().all(|dependency| dependency.dependency_name != "Std"));
    });
    drop(_core_root);
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

dependency "RemoteShared" {
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
        assert_eq!(plan.unresolved_dependencies[0].dependency_name, "RemoteShared");
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

dependency "Kernel" {
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
