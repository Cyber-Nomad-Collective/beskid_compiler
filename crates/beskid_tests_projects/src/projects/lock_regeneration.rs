//! `Project.lock` regeneration and v1 migration policy (0.6.7).
//!
//! A lock that pins nothing beyond the toolchain's Corelib is rewritten when it no longer matches
//! the project (a copied folder); `--locked` and `--frozen` never rewrite. A v1 lock is refused
//! until an explicit refresh (`beskid dev project lock`) migrates it.

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

#[test]
fn v1_lock_is_refused_until_an_explicit_refresh_migrates_it() {
    let _env = super::core_dependency_env_lock();
    let root = temp_case_dir("lock_regeneration_v1");
    let project = root.join("app");
    let manifest = write_project(&project, "app");
    let v1 = format!(
        "# Project.lock v1\nroot_manifest={}\nproject_name=app\ndependencies:\n",
        manifest.display()
    );
    fs::write(project.join(PROJECT_LOCK_FILE_NAME), &v1).expect("v1 lock");

    let error = prepare(&root, &manifest, WorkspacePrepareOptions::default()).expect_err("v1 lock must be refused");
    assert!(matches!(error, ProjectError::LockfileV1), "unexpected error: {error:?}");
    assert_eq!(error.code(), "E3024");
    let message = error.to_string();
    assert!(message.contains("v1") && message.contains("beskid dev project lock"), "{message}");
    assert_eq!(lock(&project), v1, "the refused lock was rewritten");

    prepare(&root, &manifest, WorkspacePrepareOptions { refresh_lock: true, ..Default::default() })
        .expect("refresh migrates the v1 lock");
    assert!(lock(&project).starts_with("# Project.lock v2"), "{}", lock(&project));
    prepare(&root, &manifest, WorkspacePrepareOptions::default()).expect("migrated lock is accepted");
}
