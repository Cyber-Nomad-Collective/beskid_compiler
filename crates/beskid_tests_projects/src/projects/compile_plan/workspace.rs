use std::fs;

use beskid_tests_support::{assert_same_canonical_path, temp_case_dir, write_project_manifest as write_manifest};
use beskid_analysis::projects::{
    PROJECT_LOCK_FILE_NAME, WorkspacePrepareOptions, build_compile_plan, is_project_manifest_path,
    prepare_project_workspace, prepare_project_workspace_with_options,
};

use super::super::test_cwd::with_cwd_at_workspace_root;

#[test]
fn prepare_workspace_locked_mode_accepts_semantically_equivalent_lockfile() {
    let root = temp_case_dir("workspace_prepare_locked_semantic_lock_match");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    let util_dir = root.join("Util");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");
    fs::create_dir_all(&util_dir).expect("create util dir");

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
"#,
    );
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
    fs::create_dir_all(core_dir.join("Src")).expect("create core src");
    fs::create_dir_all(util_dir.join("Src")).expect("create util src");
    fs::write(core_dir.join("Src/Core.bd"), "Fn Main() { }").expect("write core source");
    fs::write(util_dir.join("Src/Util.bd"), "Fn Main() { }").expect("write util source");

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

dependency "Util" {
  source = "path"
  path = "../Util"
}
"#,
    );
    fs::create_dir_all(app_dir.join("Src")).expect("create app src");
    fs::write(app_dir.join("Src/Main.bd"), "Fn Main() { }").expect("write app source");

    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        let workspace = prepare_project_workspace(&plan).expect("workspace should prepare");

        let lockfile_path = workspace.lockfile_path;
        let original = fs::read_to_string(&lockfile_path).expect("read lockfile");
        let mut header_lines = Vec::new();
        let mut dependency_lines = Vec::new();
        for line in original.lines() {
            if line.starts_with("- ") {
                dependency_lines.push(line.to_string());
            } else {
                header_lines.push(line.to_string());
            }
        }
        dependency_lines.reverse();
        let mut reordered = String::new();
        for line in header_lines {
            reordered.push_str(&line);
            reordered.push('\n');
        }
        for line in dependency_lines {
            reordered.push_str(&line);
            reordered.push('\n');
        }
        fs::write(&lockfile_path, reordered).expect("write reordered lockfile");

        let locked_result = prepare_project_workspace_with_options(
            &plan,
            WorkspacePrepareOptions { frozen: false, locked: true },
            None,
        );
        assert!(locked_result.is_ok());
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn prepare_project_workspace_generates_lockfile_and_materializes_dependencies() {
    let root = temp_case_dir("workspace_prepare_lock_and_materialize");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");

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
"#,
    );
    fs::create_dir_all(core_dir.join("Src")).expect("create core src dir");
    fs::write(core_dir.join("Src").join("Core.bd"), "Fn Main() { }").expect("write core source");

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
"#,
    );
    fs::create_dir_all(app_dir.join("Src")).expect("create app src dir");
    fs::write(app_dir.join("Src").join("Main.bd"), "Fn Main() { }").expect("write app source");

    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        let workspace = prepare_project_workspace(&plan).expect("workspace should prepare");

        let lockfile_path = app_dir.join(PROJECT_LOCK_FILE_NAME);
        assert!(lockfile_path.is_file());
        assert_same_canonical_path(&workspace.lockfile_path, &lockfile_path);
        assert!(!workspace.materialized_dependencies.is_empty());
        assert!(workspace.materialized_dependencies.iter().any(|dependency| dependency.dependency_name == "Core"));
        assert!(workspace.materialized_dependencies[0].materialized_source_root.is_dir());
        let lock_content = fs::read_to_string(&lockfile_path).expect("read lockfile");
        assert!(lock_content.contains("# Project.lock v1"));
        assert!(lock_content.contains("project_name=App"));
        assert!(lock_content.contains("name=Core"));

        let deps_src_root = app_dir.join("obj").join("beskid").join("deps").join("src");
        assert!(deps_src_root.is_dir());

        let mut materialized_manifest_count = 0usize;
        for entry in fs::read_dir(&deps_src_root).expect("read deps src dir") {
            let entry = entry.expect("valid deps entry");
            let dependency_root = entry.path();
            if fs::read_dir(&dependency_root)
                .into_iter()
                .flatten()
                .flatten()
                .any(|entry| entry.path().is_file() && is_project_manifest_path(&entry.path()))
            {
                materialized_manifest_count += 1;
            }
        }
        assert!(materialized_manifest_count >= 1);
    });

    let _ = fs::remove_dir_all(root);
}

#[test]
fn prepare_project_workspace_skips_obj_when_materializing_path_dependencies() {
    let root = temp_case_dir("workspace_materialize_skips_obj");
    let app_dir = root.join("App");
    let core_dir = root.join("Core");
    fs::create_dir_all(&app_dir).expect("create app dir");
    fs::create_dir_all(&core_dir).expect("create core dir");
    fs::create_dir_all(core_dir.join("obj").join("beskid").join("stale")).expect("stale obj");
    fs::write(core_dir.join("obj").join("beskid").join("stale").join("junk.txt"), "x").expect("stale obj file");
    fs::create_dir_all(core_dir.join("tests").join("nested").join("obj").join("beskid"))
        .expect("stale nested tests obj");

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
"#,
    );
    fs::create_dir_all(core_dir.join("Src")).expect("create core src dir");
    fs::write(core_dir.join("Src").join("Core.bd"), "Fn Main() { }").expect("write core source");

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
"#,
    );
    fs::create_dir_all(app_dir.join("Src")).expect("create app src dir");
    fs::write(app_dir.join("Src").join("Main.bd"), "Fn Main() { }").expect("write app source");

    let deps_src_root = app_dir.join("obj").join("beskid").join("deps").join("src");
    fs::create_dir_all(&deps_src_root).expect("create deps src root");
    let stale_materialized_core = deps_src_root.join("Core-stale");
    fs::create_dir_all(stale_materialized_core.join("obj")).expect("stale materialized obj");
    fs::create_dir_all(stale_materialized_core.join("tests").join("nested")).expect("stale materialized tests");

    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        prepare_project_workspace(&plan).expect("workspace should prepare");

        for entry in fs::read_dir(&deps_src_root).expect("read deps src dir") {
            let dependency_root = entry.expect("valid deps entry").path();
            if dependency_root.file_name().is_some_and(|name| name == "Core-stale") {
                continue;
            }
            assert!(
                !dependency_root.join("obj").exists(),
                "materialized dependency should not copy obj/: {}",
                dependency_root.display()
            );
            assert!(
                !dependency_root.join("tests").exists(),
                "materialized dependency should not copy tests/: {}",
                dependency_root.display()
            );
        }
    });

    let _ = fs::remove_dir_all(root);
}
