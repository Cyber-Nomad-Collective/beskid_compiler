//! Lockfile replay fixtures and the checked-in `sample_mod` foundation snapshot.
//!
//! Each test gets an isolated workspace that copies (or synthesizes) the fixture
//! tree under a unique temp dir so descriptors and registrations can be tweaked
//! without leaking across tests.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::projects::{
    CompilePlan, ResolvedDependencyProject, Target, TargetKind, effective_roots_from_lockfile,
    prepare_project_workspace,
};

use beskid_tests_support::temp_case_dir;

const APP_MANIFEST: &str = r#"App {
  name = "App"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "foundation" {
  source = path
  path = "../dependency"
}
"#;

const FOUNDATION_MANIFEST: &str = r#"foundation {
  name = "foundation"
  version = "0.1.0"
}

target "main" {
  kind = Lib
  entry = "Main.bd"
}
"#;

#[test]
fn sample_mod_materialized_foundation_replays_no_lossy_utf8_append_route() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/mods/sample_mod");
    let snapshots = fixture.join("obj/beskid/deps/src");
    let mut foundation_snapshots = fs::read_dir(&snapshots)
        .expect("read checked-in foundation snapshots")
        .map(|entry| entry.expect("read foundation snapshot").path())
        .filter(|path| {
            path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("corelib_foundation-"))
                && path.join("src/Core/String/String.bd").is_file()
                && path.join("src/Core/String/Utf8.bd").is_file()
        })
        .collect::<Vec<_>>();
    foundation_snapshots.sort();
    let snapshot = foundation_snapshots.first().expect("checked-in foundation source snapshot");

    let case = ReplayLockCase::new("sample_mod_foundation_v2_replay");
    let source_dir = case.plan.dependency_projects[0].source_root.join("Core/String");
    fs::create_dir_all(&source_dir).expect("create disposable foundation source");
    for source in ["String.bd", "Utf8.bd"] {
        fs::copy(snapshot.join("src/Core/String").join(source), source_dir.join(source))
            .expect("copy checked-in foundation source into disposable project");
    }
    case.prepare_lock();
    let lock = fs::read_to_string(&case.lockfile).expect("read generated v2 lockfile");
    assert!(lock.starts_with("# Project.lock v2\n"), "fixture must replay v2: {lock}");
    assert!(case.canonical_entry().contains("source=path"), "generated lock must use current graph");

    let replayed = effective_roots_from_lockfile(&case.plan, &case.lockfile);
    let expected_source_root =
        case.materialized.join("src").canonicalize().expect("canonical materialized source root");
    assert_eq!(
        replayed
            .dependencies
            .iter()
            .find(|dependency| dependency.dependency_name.as_deref() == Some("foundation"))
            .map(|dependency| dependency.source_root.as_path()),
        Some(expected_source_root.as_path()),
        "LSP lockfile replay must resolve the exact materialized v2 root"
    );

    for source in ["String.bd", "Utf8.bd"] {
        let source = fs::read_to_string(expected_source_root.join("Core/String").join(source))
            .expect("read materialized Core.String source");
        assert!(
            !source.contains("AppendUtf8Rune"),
            "materialized corelib must not reintroduce the removed lossy UTF-8 append route"
        );
    }
}

#[test]
fn lockfile_replay_fails_closed_for_invalid_or_untrusted_entries() {
    let case = ReplayLockCase::new("lock_replay_reject");
    let base = beskid_analysis::projects::effective_roots_from_plan_and_workspace(&case.plan, None);
    let valid = case.canonical_entry().to_string();
    let invalid_lockfiles = vec![
        "# Project.lock v0\n".to_string(),
        format!("# Project.lock v1\nroot_manifest=x\nproject_name=x\nnot-a-dependency\ndependencies:\n{valid}\n"),
        "# Project.lock v1\nroot_manifest=x\nproject_name=x\ndependencies:\n- name=foundation;manifest=x\n".to_string(),
        format!("{}{}", case.lock_with(&valid), valid),
        case.lock_with(&format!("{};source_root=/tampered", valid.trim_end())),
        case.lock_with(&format!("{};registry=one;registry=two", valid.trim_end())),
        format!("{}dependencies:\n", case.lock_with(&valid)),
        case.lock_with(&case.entry(&format!("{}/../escape", case.project.display()))),
        case.lock_with(&case.entry("../outside")),
    ];

    for lock in invalid_lockfiles {
        fs::write(&case.lockfile, &lock).expect("write invalid lockfile");
        assert_eq!(
            effective_roots_from_lockfile(&case.plan, &case.lockfile),
            base,
            "invalid or untrusted lockfile must not partially override roots: {lock}"
        );
    }
}

#[test]
fn lockfile_replay_rejects_contained_absolute_materialized_root() {
    let case = ReplayLockCase::new("lock_replay_absolute");
    fs::write(&case.lockfile, case.lock_with(&case.entry(&case.materialized.display().to_string())))
        .expect("write absolute lockfile");

    let replayed = effective_roots_from_lockfile(&case.plan, &case.lockfile);
    let current = beskid_analysis::projects::effective_roots_from_plan_and_workspace(&case.plan, None);
    assert_eq!(replayed, current, "absolute roots must not bypass v2 path validation");
}

struct ReplayLockCase {
    root: PathBuf,
    project: PathBuf,
    materialized: PathBuf,
    lockfile: PathBuf,
    plan: CompilePlan,
    canonical_entry: String,
}

impl ReplayLockCase {
    fn new(prefix: &str) -> Self {
        let root = temp_case_dir(prefix);
        let project = root.join("App");
        let dependency = root.join("dependency");
        fs::create_dir_all(project.join("Src")).expect("project source root");
        fs::create_dir_all(dependency.join("src")).expect("dependency source root");
        fs::write(project.join("App.bproj"), APP_MANIFEST).expect("project manifest");
        fs::write(dependency.join("Foundation.bproj"), FOUNDATION_MANIFEST).expect("dependency manifest");
        let plan = CompilePlan {
            project_root: project.clone(),
            manifest_path: project.join("App.bproj"),
            project_name: "App".to_string(),
            source_root: project.join("Src"),
            target: Target { name: "main".to_string(), kind: TargetKind::App, entry: Some("Main.bd".to_string()) },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "foundation".to_string(),
                manifest_path: dependency.join("Foundation.bproj"),
                project_root: dependency.clone(),
                project_name: "foundation".to_string(),
                source_root: dependency.join("src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        };
        let lockfile = project.join("Project.lock");
        let prepared = prepare_project_workspace(&plan).expect("prepare v2 replay fixture");
        let materialized = prepared.materialized_dependencies[0].materialized_project_root.clone();
        let lock = fs::read_to_string(&lockfile).expect("read prepared v2 lock");
        let canonical_entry = lock
            .lines()
            .find(|line| line.starts_with("- name=foundation;"))
            .expect("prepared foundation entry")
            .to_string();
        Self { lockfile, root, project, materialized, plan, canonical_entry }
    }

    fn prepare_lock(&self) {
        prepare_project_workspace(&self.plan).expect("refresh disposable materialization from current graph");
    }

    fn canonical_entry(&self) -> &str {
        &self.canonical_entry
    }

    fn relative_materialized_root(&self) -> String {
        self.materialized
            .strip_prefix(&self.project)
            .expect("materialized root below project")
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn entry(&self, materialized_root: &str) -> String {
        self.canonical_entry.replace(
            &format!("materialized_root={}", self.relative_materialized_root()),
            &format!("materialized_root={materialized_root}"),
        )
    }

    fn lock_with(&self, entry: &str) -> String {
        format!("# Project.lock v2\nroot_manifest=App.bproj\nproject_name=App\ndependencies:\n{entry}\n")
    }
}

impl Drop for ReplayLockCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
