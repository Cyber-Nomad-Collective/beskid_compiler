//! `Project.lock` regeneration policy.
//!
//! A lock that pins nothing beyond the toolchain's Corelib is rewritten when it no longer matches
//! the project (a copied folder); `--locked` and `--frozen` never rewrite. A lock in an unsupported
//! format (any first line other than `# Project.lock v3`) fails closed with E3025 in every mode.

use std::fs;
use std::path::{Path, PathBuf};

use beskid_analysis::projects::{
    PROJECT_LOCK_FILE_NAME, ProjectError, WorkspacePrepareOptions, build_compile_plan,
    prepare_project_workspace_with_options,
};
use beskid_tests_support::{temp_case_dir, write_project_manifest as write_manifest};

use super::test_cwd::with_cwd_at_workspace_root;

fn write_project(dir: &Path, name: &str) -> PathBuf {
    fs::create_dir_all(dir.join("Src")).expect("source directory");
    fs::write(dir.join("Src/Main.bd"), "Fn Main() { }\n").expect("source");
    write_manifest(
        dir,
        &format!(
            "project {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"{name}\" {{\n  kind = \"Lib\"\n  entry = \"Main.bd\"\n}}\n"
        ),
    )
}

fn prepare(root: &Path, manifest: &Path, options: WorkspacePrepareOptions) -> Result<(), ProjectError> {
    with_cwd_at_workspace_root(root, || {
        let plan = build_compile_plan(manifest, None)?;
        prepare_project_workspace_with_options(&plan, options, None).map(|_| ())
    })
}

fn lock(dir: &Path) -> String {
    fs::read_to_string(dir.join(PROJECT_LOCK_FILE_NAME)).expect("read Project.lock")
}

/// Two sibling projects, the second made by copying the first together with its lock.
fn copied_project() -> (PathBuf, PathBuf, PathBuf, PathBuf, String) {
    let root = temp_case_dir("lock_regeneration_copy");
    let first = root.join("intro1");
    let first_manifest = write_project(&first, "intro1");
    prepare(&root, &first_manifest, WorkspacePrepareOptions::default()).expect("first project lock");
    let first_lock = lock(&first);
    let second = root.join("intro2");
    let second_manifest = write_project(&second, "intro2");
    fs::write(second.join(PROJECT_LOCK_FILE_NAME), &first_lock).expect("copied lock");
    (root, first, second, second_manifest, first_lock)
}

#[test]
fn pin_free_lock_of_a_copied_project_is_regenerated() {
    let _env = super::core_dependency_env_lock();
    let (root, _first, second, second_manifest, first_lock) = copied_project();
    assert!(first_lock.contains("project_name=intro1"), "{first_lock}");

    prepare(&root, &second_manifest, WorkspacePrepareOptions::default()).expect("copied pin-free lock regenerates");

    let regenerated = lock(&second);
    assert!(regenerated.contains("project_name=intro2"), "{regenerated}");
    assert!(!regenerated.contains("intro1"), "{regenerated}");
}

/// Readers (the language server's lock replay, the project explorer) agree with preparation: a
/// copied lock that pins only Corelib is not an error, it is read as absent until regenerated.
#[test]
fn readers_do_not_report_a_copied_pin_free_lock() {
    let _env = super::core_dependency_env_lock();
    let (root, _first, second, second_manifest, first_lock) = copied_project();
    with_cwd_at_workspace_root(&root, || {
        let plan = build_compile_plan(&second_manifest, None).expect("plan");
        let lock_path = second.join(PROJECT_LOCK_FILE_NAME);
        let entries = beskid_analysis::projects::load_project_lock_dependencies_for_plan(&lock_path, &plan)
            .expect("a copied pin-free lock is not an error for readers");
        assert!(entries.is_empty(), "{entries:?}");
        beskid_analysis::projects::effective_roots_from_lockfile_checked(&plan, &lock_path)
            .expect("lock replay falls back to the declared roots");
    });
    assert_eq!(lock(&second), first_lock, "readers never rewrite a lock");
}

#[test]
fn locked_and_frozen_runs_never_rewrite_a_foreign_lock() {
    let _env = super::core_dependency_env_lock();
    let (root, _first, second, second_manifest, first_lock) = copied_project();
    for options in [
        WorkspacePrepareOptions { locked: true, ..Default::default() },
        WorkspacePrepareOptions { frozen: true, locked: true, offline: true, ..Default::default() },
    ] {
        let error = prepare(&root, &second_manifest, options).expect_err("a locked run must keep the foreign lock");
        assert!(error.to_string().contains("different project"), "unexpected error: {error}");
        assert_eq!(lock(&second), first_lock, "a locked run rewrote the lock");
    }
}

/// A lock whose first line is not the format 3 header (an older format or an unknown one) fails
/// closed with E3025 in every mode, the explicit refresh of `beskid dev project lock` included:
/// nothing reads, regenerates or rewrites it, and preparation writes nothing. Once the user deletes
/// it, the next preparation writes format 3.
#[test]
fn lock_in_an_unsupported_format_fails_closed_until_deleted() {
    let _env = super::core_dependency_env_lock();
    let root = temp_case_dir("lock_regeneration_unsupported");
    let project = root.join("app");
    let manifest = write_project(&project, "app");
    let corelib_only = "root_manifest=app.bproj\nproject_name=app\ndependencies:\n- name=Core;source=corelib;project=beskid_corelib;manifest=corelib.bproj;source_root=.\n";
    let headers = ["# Project.lock v1", "# Project.lock v2", "# Project.lock v9", "Project.lock v3"];
    let modes = [
        WorkspacePrepareOptions::default(),
        WorkspacePrepareOptions { refresh_lock: true, ..Default::default() },
        WorkspacePrepareOptions { locked: true, ..Default::default() },
        WorkspacePrepareOptions { frozen: true, locked: true, offline: true, ..Default::default() },
    ];
    for header in headers {
        let old = format!("{header}\n{corelib_only}");
        fs::write(project.join(PROJECT_LOCK_FILE_NAME), &old).expect("old lock");
        for options in modes {
            let error = prepare(&root, &manifest, options).expect_err("an unsupported lock must be refused");
            assert!(
                matches!(&error, ProjectError::LockfileFormatUnsupported { path } if path.ends_with(PROJECT_LOCK_FILE_NAME)),
                "{header} {options:?}: unexpected error: {error:?}"
            );
            assert_eq!(error.code(), "E3025");
            let message = error.to_string();
            assert!(message.contains("delete the file") && message.contains("beskid dev project lock"), "{message}");
            assert_eq!(lock(&project), old, "{header} {options:?}: the refused lock was rewritten");
            assert!(!project.join("obj").exists(), "{header} {options:?}: preparation wrote output");
        }
    }

    fs::remove_file(project.join(PROJECT_LOCK_FILE_NAME)).expect("delete the old lock");
    prepare(&root, &manifest, WorkspacePrepareOptions::default()).expect("a deleted lock is written again");
    assert!(lock(&project).starts_with("# Project.lock v3\n"), "{}", lock(&project));
}
