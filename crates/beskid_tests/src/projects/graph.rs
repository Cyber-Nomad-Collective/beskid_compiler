use std::fs;

use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};
use beskid_analysis::projects::{
    UnresolvedDependencyKind, build_project_graph, collect_dependency_projects, collect_unresolved_dependencies,
};

use super::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};

#[test]
fn collect_unresolved_dependencies_reports_git_and_registry_nodes() {
    let dir = temp_case_dir("unresolved_nodes");
    let source = r#"
project {
  name = "App"
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

dependency "PkgCore" {
  source = "registry"
  version = "1.2.3"
}
"#;
    let manifest_path = write_manifest(&dir, source);

    with_cwd_at_workspace_root(&dir, || {
        let graph = build_project_graph(&manifest_path).expect("graph should build");
        let unresolved = collect_unresolved_dependencies(&graph);
        assert_eq!(unresolved.len(), 2);

        let git =
            unresolved.iter().find(|note| note.kind == UnresolvedDependencyKind::Git).expect("git unresolved dep");
        assert_eq!(git.dependency_name, "RemoteStd");
        assert_eq!(git.descriptor, "git@example.com/std.git@abc123");

        let registry = unresolved
            .iter()
            .find(|note| note.kind == UnresolvedDependencyKind::Registry)
            .expect("registry unresolved dep");
        assert_eq!(registry.dependency_name, "PkgCore");
        assert_eq!(registry.descriptor, "1.2.3");
    });

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn collect_dependency_projects_is_dependency_first_and_deduplicated() {
    let root = temp_case_dir("dependency_projection_order");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    let feature_dir = root.join("Feature");
    let util_dir = root.join("Util");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");
    fs::create_dir_all(&feature_dir).expect("create feature dir");
    fs::create_dir_all(&util_dir).expect("create util dir");

    write_manifest(
        &util_dir,
        r#"
project {
  name = "Util"
  version = "0.1.0"
}

target "UtilLib" {
  kind = "Lib"
  entry = "Util.bd"
}
"#,
    );

    write_manifest(
        &core_dir,
        r#"
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
"#,
    );

    write_manifest(
        &feature_dir,
        r#"
project {
  name = "Feature"
  version = "0.1.0"
}

target "FeatureLib" {
  kind = "Lib"
  entry = "Feature.bd"
}

dependency "Util" {
  source = "path"
  path = "../Util"
}
"#,
    );

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

dependency "Core" {
  source = "path"
  path = "../Core"
}

dependency "Feature" {
  source = "path"
  path = "../Feature"
}
"#,
    );

    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let graph = build_project_graph(&app_manifest_path).expect("graph should build");
        let deps = collect_dependency_projects(&graph);
        let names: Vec<_> = deps.iter().map(|d| d.dependency_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "corelib_compiler_sdk",
                "corelib_foundation",
                "corelib_concurrency",
                "corelib_runtime",
                "corelib_console",
                "Std",
                "Util",
                "Core",
                "Feature",
            ],
            "dependency order: {names:?}"
        );
    });

    let _ = fs::remove_dir_all(root);
}
