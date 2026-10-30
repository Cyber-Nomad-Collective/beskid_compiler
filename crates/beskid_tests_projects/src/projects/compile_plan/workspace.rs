use std::fs;
use std::path::{Path, PathBuf};

use beskid_abi::corelib_bundle::{CORELIB_BUNDLE_FINGERPRINT_FILE, fingerprint_corelib_bundle_dir};
use beskid_analysis::projects::{
    CompilePlan, PROJECT_LOCK_FILE_NAME, ProjectError, ProjectGraphBuildOptions, UnresolvedDependencyPolicy,
    WorkspacePrepareOptions, build_compile_plan, build_compile_plan_with_policy_and_graph, is_project_manifest_path,
    prepare_project_workspace, prepare_project_workspace_with_options,
};
use beskid_tests_support::{assert_same_canonical_path, temp_case_dir, write_project_manifest as write_manifest};

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

/// A compile plan whose implicit `Core` comes from the Corelib installed at `corelib_root`.
fn plan_with_corelib(manifest: &Path, corelib_root: &Path) -> CompilePlan {
    build_compile_plan_with_policy_and_graph(
        manifest,
        None,
        UnresolvedDependencyPolicy::Error,
        ProjectGraphBuildOptions::with_corelib_root(corelib_root),
    )
    .expect("plan with an installed Corelib")
}

fn dependency_project_roots(workspace: &beskid_analysis::projects::PreparedProjectWorkspace) -> Vec<PathBuf> {
    workspace.materialized_dependencies.iter().map(|dependency| dependency.materialized_project_root.clone()).collect()
}

fn dependency_copies(project_dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(project_dir.join("obj/beskid/deps/src"))
        .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).collect())
        .unwrap_or_default()
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
    let original_parent = temp_case_dir("portable_sibling_original");
    let moved_parent = temp_case_dir("portable_sibling_moved");
    let original_tree = original_parent.join("tree");
    let app_dir = original_tree.join("App");
    write_portable_test_project(&original_tree.join("Sibling"), "Sibling", None);
    let app_manifest = write_portable_test_project(&app_dir, "App", Some(("Sibling", "../Sibling")));

    let (original_lock, original_names) = {
        let plan = build_compile_plan(&app_manifest, None).expect("original plan");
        let workspace = prepare_project_workspace(&plan).expect("original workspace");
        (fs::read(&workspace.lockfile_path).expect("read original lock"), materialized_dependency_names(&workspace))
    };
    assert!(original_lock.starts_with(b"# Project.lock v3\n"));
    assert!(String::from_utf8_lossy(&original_lock).contains("source=path"));
    assert!(!original_names.is_empty());

    let moved_tree = moved_parent.join("tree");
    fs::rename(&original_tree, &moved_tree).expect("move complete fixture tree");
    let moved_app = moved_tree.join("App");
    let manifest_name = app_manifest.file_name().expect("app manifest file name");
    let (moved_lock, moved_names) = {
        let plan = build_compile_plan(&moved_app.join(manifest_name), None).expect("relocated plan");
        let workspace = prepare_project_workspace_with_options(
            &plan,
            WorkspacePrepareOptions { offline: false, frozen: false, locked: true, refresh_lock: false },
            None,
        )
        .expect("preserved sibling layout must reuse the committed lock");
        (fs::read(&workspace.lockfile_path).expect("read relocated lock"), materialized_dependency_names(&workspace))
    };
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

    {
        let plan = build_compile_plan(&app_manifest, None).expect("original plan");
        prepare_project_workspace(&plan).expect("write original lock");
    };

    let moved_tree = moved_parent.join("tree");
    fs::create_dir_all(&moved_tree).expect("create relocated parent");
    fs::rename(&app_dir, moved_tree.join("App")).expect("move app without its declared sibling");
    assert!(moved_tree.join("App").join(PROJECT_LOCK_FILE_NAME).is_file());
    assert!(!moved_tree.join("Sibling").exists());
    let manifest_name = app_manifest.file_name().expect("app manifest file name");
    let error = {
        build_compile_plan(&moved_tree.join("App").join(manifest_name), None)
            .expect_err("missing declared path must fail")
    };
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

/// Two toolchains side by side each have their own sealed Corelib. A project prepared by either
/// reads Corelib in place from that toolchain's bundle: no copy lands in `obj/beskid/deps`, the
/// lock bytes do not depend on which toolchain wrote them, and the shared bundle is never written.
#[test]
fn distinct_installed_corelib_roots_keep_lock_bytes_and_materialized_names() {
    let fixture = temp_case_dir("portable_corelib_roots");
    let app_dir = fixture.join("App");
    let app_manifest = write_portable_test_project(&app_dir, "App", None);
    let first_corelib = fixture.join("first-install");
    let second_corelib = fixture.join("second-install");
    for install in [&first_corelib, &second_corelib] {
        write_portable_test_project(&install.join("beskid_corelib"), "corelib", None);
        // A verified installed Corelib carries every member its workspace manifest lists.
        fs::write(install.join("CoreLib.bws"), "workspace {\n  name = \"corelib\"\n  resolver = v1\n  schema = \"beskid.workspace.v1\"\n}\n\nmember \"corelib\" {\n  path = \"beskid_corelib\"\n}\n")
            .expect("write installed Corelib workspace manifest");
        let fingerprint = fingerprint_corelib_bundle_dir(install).expect("fingerprint installed Corelib fixture");
        fs::write(install.join(CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n"))
            .expect("mark verified Corelib fixture");
    }

    let sealed_before = [&first_corelib, &second_corelib]
        .map(|install| fingerprint_corelib_bundle_dir(install).expect("fingerprint before preparation"));
    // A stale Corelib copy left by an older toolchain is removed by the next preparation.
    let stale_copy = app_dir.join("obj/beskid/deps/src/Core-81dbe9b059abfcd73535c29e5d790e42");
    fs::create_dir_all(stale_copy.join("Src")).expect("stale Corelib copy");
    fs::write(stale_copy.join("Src/Old.bd"), "pub i32 Old() { return 0; }\n").expect("stale Corelib source");

    let (first_lock, first_names, first_roots) = {
        {
            let plan = plan_with_corelib(&app_manifest, &first_corelib);
            assert!(plan.has_core_dependency, "fixture must resolve implicit Core");
            let workspace = prepare_project_workspace(&plan).expect("workspace with first installed Corelib");
            (
                fs::read(&workspace.lockfile_path).expect("read first lock"),
                materialized_dependency_names(&workspace),
                dependency_project_roots(&workspace),
            )
        }
    };
    assert!(first_lock.starts_with(b"# Project.lock v3\n"));
    let first_text = String::from_utf8_lossy(&first_lock).into_owned();
    assert!(first_text.contains("source=corelib"), "{first_text}");
    assert!(!first_text.contains("materialized_root"), "a Corelib entry records no copy: {first_text}");
    assert!(!first_names.is_empty());
    let first_install = first_corelib.canonicalize().expect("canonical first install");
    assert!(first_roots.iter().all(|root| root.starts_with(&first_install)), "{first_roots:?}");
    assert!(dependency_copies(&app_dir).is_empty(), "no Corelib copy: {:?}", dependency_copies(&app_dir));

    let (second_lock, second_names, second_roots) = {
        {
            let plan = plan_with_corelib(&app_manifest, &second_corelib);
            assert!(plan.has_core_dependency, "fixture must resolve implicit Core");
            let workspace = prepare_project_workspace_with_options(
                &plan,
                WorkspacePrepareOptions { offline: false, frozen: false, locked: true, refresh_lock: false },
                None,
            )
            .expect("the same lock must accept another installed Corelib root");
            (
                fs::read(&workspace.lockfile_path).expect("read second lock"),
                materialized_dependency_names(&workspace),
                dependency_project_roots(&workspace),
            )
        }
    };
    let second_install = second_corelib.canonicalize().expect("canonical second install");
    assert!(second_roots.iter().all(|root| root.starts_with(&second_install)), "{second_roots:?}");
    assert!(dependency_copies(&app_dir).is_empty());
    let sealed_after = [&first_corelib, &second_corelib]
        .map(|install| fingerprint_corelib_bundle_dir(install).expect("fingerprint after preparation"));
    assert_eq!(sealed_after, sealed_before, "preparation must never write into a shared Corelib");
    assert_eq!(second_lock, first_lock, "Corelib installation path must not rewrite Project.lock");
    assert_eq!(second_names, first_names, "Corelib installation path must not rename materialized dependencies");

    let _ = fs::remove_dir_all(fixture);
}

/// The shared Corelib is read-only: preparation fails closed (E3032) once a sealed bundle no longer
/// matches its fingerprint, and it never repairs or rewrites the bundle.
#[test]
fn modified_shared_corelib_fails_closed() {
    let fixture = temp_case_dir("shared_corelib_modified");
    let app_manifest = write_portable_test_project(&fixture.join("App"), "App", None);
    let install = fixture.join("install");
    write_portable_test_project(&install.join("beskid_corelib"), "corelib", None);
    fs::write(install.join("CoreLib.bws"), "workspace {\n  name = \"corelib\"\n  resolver = v1\n  schema = \"beskid.workspace.v1\"\n}\n\nmember \"corelib\" {\n  path = \"beskid_corelib\"\n}\n")
        .expect("write installed Corelib workspace manifest");
    let fingerprint = fingerprint_corelib_bundle_dir(&install).expect("fingerprint installed Corelib fixture");
    fs::write(install.join(CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n")).expect("seal fixture");

    {
        let plan = plan_with_corelib(&app_manifest, &install);
        prepare_project_workspace(&plan).expect("an intact shared Corelib prepares");

        let edited = install.join("beskid_corelib/Src/Main.bd");
        fs::write(&edited, "Fn Main() { Tampered(); }\n").expect("edit the shared Corelib");
        let error = prepare_project_workspace(&plan).expect_err("a modified shared Corelib must fail closed");
        assert!(matches!(error, ProjectError::SharedCorelibModified { .. }), "unexpected error: {error:?}");
        assert_eq!(error.code(), "E3032");
        assert_eq!(fs::read_to_string(&edited).unwrap(), "Fn Main() { Tampered(); }\n", "never repaired");
    };
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

dependency "Kernel" {
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

    {
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
        assert_ne!(reordered, original, "the fixture lock has entries to reorder");
        fs::write(&lockfile_path, &reordered).expect("write reordered lockfile");

        let locked_result = prepare_project_workspace_with_options(
            &plan,
            WorkspacePrepareOptions { offline: false, frozen: false, locked: true, refresh_lock: false },
            None,
        );
        assert!(locked_result.is_ok());
        assert_eq!(fs::read_to_string(&lockfile_path).expect("read lockfile"), reordered, "locked keeps it");

        // An ordinary run writes the equivalent lock back in the canonical order.
        prepare_project_workspace(&plan).expect("ordinary prepare");
        assert_eq!(fs::read_to_string(&lockfile_path).expect("read lockfile"), original, "canonicalized");
    };

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

dependency "Kernel" {
  source = "path"
  path = "../Core"
}
"#,
    );
    fs::create_dir_all(app_dir.join("Src")).expect("create app src dir");
    fs::write(app_dir.join("Src").join("Main.bd"), "Fn Main() { }").expect("write app source");

    {
        let plan = build_compile_plan(&app_manifest_path, None).expect("plan should build");
        let workspace = prepare_project_workspace(&plan).expect("workspace should prepare");

        let lockfile_path = app_dir.join(PROJECT_LOCK_FILE_NAME);
        assert!(lockfile_path.is_file());
        assert_same_canonical_path(&workspace.lockfile_path, &lockfile_path);
        assert!(!workspace.materialized_dependencies.is_empty());
        assert!(workspace.materialized_dependencies.iter().any(|dependency| dependency.dependency_name == "Kernel"));
        assert!(workspace.materialized_dependencies[0].materialized_source_root.is_dir());
        let lock_content = fs::read_to_string(&lockfile_path).expect("read lockfile");
        assert!(lock_content.starts_with("# Project.lock v3\n"));
        assert!(lock_content.contains("project_name=App"));
        assert!(lock_content.contains("name=Kernel"));

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
    };

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

dependency "Kernel" {
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

    {
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
    };

    let _ = fs::remove_dir_all(root);
}
