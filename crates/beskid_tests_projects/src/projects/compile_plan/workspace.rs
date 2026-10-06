use std::fs;
use std::path::{Path, PathBuf};

use beskid_abi::corelib_bundle::{CORELIB_BUNDLE_FINGERPRINT_FILE, fingerprint_corelib_bundle_dir};
use beskid_analysis::projects::{
    PROJECT_LOCK_FILE_NAME, ProjectError, WorkspacePrepareOptions, build_compile_plan, is_project_manifest_path,
    prepare_project_workspace, prepare_project_workspace_with_options,
};
use beskid_tests_support::{assert_same_canonical_path, temp_case_dir, write_project_manifest as write_manifest};

use super::super::test_cwd::with_cwd_at_workspace_root;

fn write_portable_test_project(project_dir: &Path, name: &str, dependency: Option<(&str, &str)>) -> PathBuf {
    fs::create_dir_all(project_dir.join("Src")).expect("create project source dir");
    fs::write(project_dir.join("Src/Main.bd"), "Fn Main() { }\n").expect("write project source");
    let mut manifest = format!(
        "project {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"{name}\" {{\n  kind = \"Lib\"\n  entry = \"Main.bd\"\n}}\n"
    );
    if let Some((dependency_name, dependency_path)) = dependency {
        manifest.push_str(&format!(
            "\ndependency \"{dependency_name}\" {{\n  source = \"path\"\n  path = \"{dependency_path}\"\n}}\n"
        ));
    }
    write_manifest(project_dir, &manifest)
}

fn materialized_dependency_names(workspace: &beskid_analysis::projects::PreparedProjectWorkspace) -> Vec<String> {
    let mut names = workspace
        .materialized_dependencies
        .iter()
        .map(|dependency| {
            dependency
                .materialized_project_root
                .file_name()
                .expect("materialized dependency directory name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn relocated_sibling_path_keeps_lock_bytes_and_materialized_names() {
    // Keep the implicit Std root stable across both plans while other project tests
    // exercise temporary Corelib installations; acquire this before the cwd lock.
    let _env = super::super::std_dependency_env_lock();
    let original_parent = temp_case_dir("portable_sibling_original");
    let moved_parent = temp_case_dir("portable_sibling_moved");
    let original_tree = original_parent.join("tree");
    let app_dir = original_tree.join("App");
    write_portable_test_project(&original_tree.join("Sibling"), "Sibling", None);
    let app_manifest = write_portable_test_project(&app_dir, "App", Some(("Sibling", "../Sibling")));

    let (original_lock, original_names) = with_cwd_at_workspace_root(&original_tree, || {
        let plan = build_compile_plan(&app_manifest, None).expect("original plan");
        let workspace = prepare_project_workspace(&plan).expect("original workspace");
        (fs::read(&workspace.lockfile_path).expect("read original lock"), materialized_dependency_names(&workspace))
    });
    assert!(original_lock.starts_with(b"# Project.lock v2\n"));
    assert!(String::from_utf8_lossy(&original_lock).contains("source=path"));
    assert!(!original_names.is_empty());

    let moved_tree = moved_parent.join("tree");
    fs::rename(&original_tree, &moved_tree).expect("move complete fixture tree");
    let moved_app = moved_tree.join("App");
    let manifest_name = app_manifest.file_name().expect("app manifest file name");
    let (moved_lock, moved_names) = with_cwd_at_workspace_root(&moved_tree, || {
        let plan = build_compile_plan(&moved_app.join(manifest_name), None).expect("relocated plan");
        let workspace = prepare_project_workspace_with_options(
            &plan,
            WorkspacePrepareOptions { offline: false, frozen: false, locked: true, refresh_lock: false },
            None,
        )
        .expect("preserved sibling layout must reuse the committed lock");
        (fs::read(&workspace.lockfile_path).expect("read relocated lock"), materialized_dependency_names(&workspace))
    });
    assert_eq!(moved_lock, original_lock, "relocation must not rewrite Project.lock");
    assert_eq!(moved_names, original_names, "source relocation must not rename materialized dependencies");

    let _ = fs::remove_dir_all(original_parent);
    let _ = fs::remove_dir_all(moved_parent);
}

#[test]
fn relocated_path_with_missing_declared_sibling_fails_closed() {
    let original_parent = temp_case_dir("portable_missing_sibling_original");
    let moved_parent = temp_case_dir("portable_missing_sibling_moved");
    let original_tree = original_parent.join("tree");
    let app_dir = original_tree.join("App");
    write_portable_test_project(&original_tree.join("Sibling"), "Sibling", None);
    let app_manifest = write_portable_test_project(&app_dir, "App", Some(("Sibling", "../Sibling")));

    with_cwd_at_workspace_root(&original_tree, || {
        let plan = build_compile_plan(&app_manifest, None).expect("original plan");
        prepare_project_workspace(&plan).expect("write original lock");
    });

    let moved_tree = moved_parent.join("tree");
    fs::create_dir_all(&moved_tree).expect("create relocated parent");
    fs::rename(&app_dir, moved_tree.join("App")).expect("move app without its declared sibling");
    assert!(moved_tree.join("App").join(PROJECT_LOCK_FILE_NAME).is_file());
    assert!(!moved_tree.join("Sibling").exists());
    let manifest_name = app_manifest.file_name().expect("app manifest file name");
    let error = with_cwd_at_workspace_root(&moved_tree, || {
        build_compile_plan(&moved_tree.join("App").join(manifest_name), None)
            .expect_err("missing declared path must fail")
    });
    assert!(
        matches!(
            &error,
            ProjectError::ReadManifest { path, source }
                if path.file_name().is_some_and(|name| name == "Sibling")
                    && source.kind() == std::io::ErrorKind::NotFound
        ) || matches!(&error, ProjectError::DependencyManifestNotFound { dependency, .. } if dependency == "Sibling"),
        "unexpected error: {error}"
    );

    let _ = fs::remove_dir_all(original_parent);
    let _ = fs::remove_dir_all(moved_parent);
}

#[test]
fn distinct_installed_corelib_roots_keep_lock_bytes_and_materialized_names() {
    let fixture = temp_case_dir("portable_corelib_roots");
    let app_dir = fixture.join("App");
    let app_manifest = write_portable_test_project(&app_dir, "App", None);
    let first_corelib = fixture.join("first-install");
    let second_corelib = fixture.join("second-install");
    for install in [&first_corelib, &second_corelib] {
        write_portable_test_project(&install.join("beskid_corelib"), "Std", None);
        let fingerprint = fingerprint_corelib_bundle_dir(install).expect("fingerprint installed Corelib fixture");
        fs::write(install.join(CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n"))
            .expect("mark verified Corelib fixture");
    }

    let (first_lock, first_names) = {
        let _corelib_root = super::super::scoped_std_dependency_root(&first_corelib);
        with_cwd_at_workspace_root(&fixture, || {
            let plan = build_compile_plan(&app_manifest, None).expect("plan with first installed Corelib");
            assert!(plan.has_std_dependency, "fixture must resolve implicit Std");
            let workspace = prepare_project_workspace(&plan).expect("workspace with first installed Corelib");
            (fs::read(&workspace.lockfile_path).expect("read first lock"), materialized_dependency_names(&workspace))
        })
    };
    assert!(first_lock.starts_with(b"# Project.lock v2\n"));
    assert!(String::from_utf8_lossy(&first_lock).contains("source=corelib"));
    assert!(!first_names.is_empty());

    let (second_lock, second_names) = {
        let _corelib_root = super::super::scoped_std_dependency_root(&second_corelib);
        with_cwd_at_workspace_root(&fixture, || {
            let plan = build_compile_plan(&app_manifest, None).expect("plan with second installed Corelib");
            assert!(plan.has_std_dependency, "fixture must resolve implicit Std");
            let workspace = prepare_project_workspace_with_options(
                &plan,
                WorkspacePrepareOptions { offline: false, frozen: false, locked: true, refresh_lock: false },
                None,
            )
            .expect("the same lock must accept another installed Corelib root");
            (fs::read(&workspace.lockfile_path).expect("read second lock"), materialized_dependency_names(&workspace))
        })
    };
    assert_eq!(second_lock, first_lock, "Corelib installation path must not rewrite Project.lock");
    assert_eq!(second_names, first_names, "Corelib installation path must not rename materialized dependencies");

    let _ = fs::remove_dir_all(fixture);
}

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
            WorkspacePrepareOptions { offline: false, frozen: false, locked: true, refresh_lock: false },
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
        assert!(lock_content.starts_with("# Project.lock v2\n"));
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
            // `tests/` is ordinary package content; only the nested build output inside it is private.
            assert!(
                !dependency_root.join("tests").join("nested").join("obj").exists(),
                "materialized dependency should not copy nested obj/: {}",
                dependency_root.display()
            );
        }
    });

    let _ = fs::remove_dir_all(root);
}
