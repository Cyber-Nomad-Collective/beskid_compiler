use std::fs;
use std::path::{Path, PathBuf};

use beskid_analysis::projects::{
    CompilePlan, PROJECT_LOCK_FILE_NAME, build_compile_plan, effective_roots_from_lockfile, prepare_project_workspace,
};
use beskid_tests_support::{temp_case_dir, write_project_manifest};

use super::super::test_cwd::with_cwd_at_workspace_root;

struct ReplayFixture {
    root: PathBuf,
    plan: CompilePlan,
    lock_path: PathBuf,
    materialized_source: PathBuf,
}

impl ReplayFixture {
    fn new(label: &str, dependency_dir: &str) -> Self {
        let root = temp_case_dir(label);
        let app = root.join("App");
        let dependency = root.join(dependency_dir);
        let dependency_manifest = format!(
            "project {{\n  name = \"Shared\"\n  version = \"0.1.0\"\n}}\n\ntarget \"Shared\" {{\n  kind = \"Lib\"\n  entry = \"Main.bd\"\n}}\n"
        );
        write_project(&dependency, &dependency_manifest);
        let app_manifest = format!(
            "project {{\n  name = \"App\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = \"App\"\n  entry = \"Main.bd\"\n}}\n\ndependency \"Shared\" {{\n  source = \"path\"\n  path = \"../{dependency_dir}\"\n}}\n"
        );
        let manifest_path = write_project(&app, &app_manifest);
        let (plan, materialized_source) = with_cwd_at_workspace_root(&root, || {
            let plan = build_compile_plan(&manifest_path, None).expect("resolve fixture graph");
            let workspace = prepare_project_workspace(&plan).expect("write genuine v2 lock and materialize graph");
            let source = workspace
                .materialized_dependencies
                .iter()
                .find(|entry| entry.dependency_name == "Shared")
                .expect("materialized Shared")
                .materialized_source_root
                .clone();
            (plan, source)
        });
        let lock_path = app.join(PROJECT_LOCK_FILE_NAME);
        let lock = fs::read_to_string(&lock_path).expect("read generated lock");
        assert!(lock.starts_with("# Project.lock v2\n"), "fixture must exercise v2 replay");
        Self { root, plan, lock_path, materialized_source }
    }

    fn replayed_source(&self) -> PathBuf {
        let roots = effective_roots_from_lockfile(&self.plan, &self.lock_path);
        roots
            .dependencies
            .iter()
            .find(|entry| entry.dependency_name.as_deref() == Some("Shared"))
            .expect("Shared effective root")
            .source_root
            .clone()
    }

    fn assert_valid_replay(&self) {
        let actual = self.replayed_source().canonicalize().expect("resolve replayed materialized source");
        let expected = self.materialized_source.canonicalize().expect("resolve expected materialized source");
        assert_eq!(actual, expected, "a genuine v2 lock must replay the current graph's materialized source");
    }

    fn assert_fallback(&self) {
        let original = self.plan.dependency_projects.iter().find(|entry| entry.dependency_name == "Shared").unwrap();
        let actual = self.replayed_source().canonicalize().expect("resolve fallback source");
        let expected = original.source_root.canonicalize().expect("resolve declared source");
        assert_eq!(
            actual, expected,
            "an invalid lock must fall back to the graph's declared source, never a lock token"
        );
    }

    fn rewrite_lock(&self, change: impl FnOnce(String) -> String) {
        let original = fs::read_to_string(&self.lock_path).expect("read generated lock");
        let updated = change(original.clone());
        assert_ne!(updated, original, "tamper must alter the lock");
        fs::write(&self.lock_path, updated).expect("write tampered lock");
    }
}

impl Drop for ReplayFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_project(root: &Path, manifest: &str) -> PathBuf {
    fs::create_dir_all(root.join("Src")).expect("create project source");
    fs::write(root.join("Src/Main.bd"), "Fn Main() { }\n").expect("write project source");
    write_project_manifest(root, manifest)
}

fn materialized_root_from_lock(lock: &str) -> String {
    let line = lock.lines().find(|line| line.starts_with("- name=Shared;")).expect("Shared lock entry");
    line.split(';').find_map(|field| field.strip_prefix("materialized_root=")).expect("materialized root").to_string()
}

#[test]
fn v2_replay_uses_current_materialized_graph() {
    ReplayFixture::new("v2_replay_valid", "Sibling").assert_valid_replay();
}

#[test]
fn v2_replay_rejects_copied_same_name_lock_with_different_manifest_graph() {
    let first = ReplayFixture::new("v2_replay_first_graph", "FirstSibling");
    let second = ReplayFixture::new("v2_replay_second_graph", "SecondSibling");
    second.assert_valid_replay();
    let copied = fs::read(&first.lock_path).expect("read first lock");
    fs::write(&second.lock_path, copied).expect("copy lock with same names but different declared paths");
    second.assert_fallback();
}

#[test]
fn v2_replay_rejects_swapped_dependency_manifest() {
    let fixture = ReplayFixture::new("v2_replay_swapped_manifest", "Sibling");
    fixture.assert_valid_replay();
    let dependency = fixture.plan.dependency_projects.iter().find(|entry| entry.dependency_name == "Shared").unwrap();
    let other_manifest = dependency.project_root.join("Other.bproj");
    fs::copy(&dependency.manifest_path, &other_manifest).expect("copy a different manifest at the same project root");
    let original_name = dependency.manifest_path.file_name().unwrap().to_string_lossy();
    fixture.rewrite_lock(|lock| lock.replace(&format!(";manifest={original_name};"), ";manifest=Other.bproj;"));
    fixture.assert_fallback();
}

#[test]
fn v2_replay_falls_back_when_materialized_dependency_is_missing() {
    let fixture = ReplayFixture::new("v2_replay_missing_materialization", "Sibling");
    fixture.assert_valid_replay();
    let parent = fixture.materialized_source.parent().expect("materialized project root");
    fs::remove_dir_all(parent).expect("remove only this disposable test materialization");
    fixture.assert_fallback();
}

#[test]
fn v2_replay_rejects_outside_root_materialized_token() {
    let fixture = ReplayFixture::new("v2_replay_outside_root", "Sibling");
    fixture.assert_valid_replay();
    fixture.rewrite_lock(|lock| {
        let materialized = materialized_root_from_lock(&lock);
        lock.replace(&format!("materialized_root={materialized}"), "materialized_root=../outside")
    });
    fixture.assert_fallback();
}

#[cfg(unix)]
#[test]
fn v2_replay_rejects_materialized_symlink_escape() {
    use std::os::unix::fs::symlink;

    let fixture = ReplayFixture::new("v2_replay_symlink_escape", "Sibling");
    fixture.assert_valid_replay();
    let materialized_root = fixture.materialized_source.parent().expect("materialized project root");
    let outside = fixture.root.join("outside");
    fs::rename(materialized_root, &outside).expect("move disposable materialization outside the owned prefix");
    symlink(&outside, materialized_root).expect("replace materialization with escaping symlink");
    fixture.assert_fallback();
}

#[cfg(unix)]
#[test]
fn v2_replay_rejects_symlinked_trusted_dependencies_prefix() {
    use std::os::unix::fs::symlink;

    let fixture = ReplayFixture::new("v2_replay_symlink_prefix", "Sibling");
    fixture.assert_valid_replay();
    let dependencies_root = fixture.plan.project_root.join("obj/beskid/deps/src");
    let outside = fixture.root.join("outside-deps");
    fs::rename(&dependencies_root, &outside).expect("move disposable materialization outside the project");
    symlink(&outside, &dependencies_root).expect("replace trusted prefix with escaping symlink");
    fixture.assert_fallback();
}

#[test]
fn v2_replay_rejects_forged_corelib_source() {
    let fixture = ReplayFixture::new("v2_replay_forged_corelib", "Sibling");
    fixture.assert_valid_replay();
    fixture.rewrite_lock(|lock| {
        lock.replace(";source=path;project=../Sibling;", ";source=corelib;project=packages/foundation;")
    });
    fixture.assert_fallback();
}

#[test]
fn v2_replay_rejects_duplicate_materialized_destination() {
    let fixture = ReplayFixture::new("v2_replay_duplicate_destination", "Sibling");
    fixture.assert_valid_replay();
    fixture.rewrite_lock(|lock| {
        let original = lock.lines().find(|line| line.starts_with("- name=Shared;")).expect("Shared lock entry");
        format!("{lock}{}\n", original.replace("name=Shared;", "name=Other;"))
    });
    fixture.assert_fallback();
}
