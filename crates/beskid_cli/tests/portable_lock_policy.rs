//! End-to-end policy checks at the `beskid` command boundary.
//!
//! These fixtures intentionally use only local path dependencies. A rejected lock must fail
//! during project preparation, before build, run, or test can reach code generation.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct ProjectCase {
    root: PathBuf,
    app: PathBuf,
    lock: PathBuf,
}

impl ProjectCase {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let root = std::env::temp_dir().join(format!(
            "beskid_portable_lock_policy_{}_{}_{}",
            std::process::id(),
            nonce,
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        let app = root.join("App");
        let core = root.join("Core");
        let other = root.join("Other");
        fs::create_dir_all(app.join("Src")).expect("app source directory");
        for project in [&core, &other] {
            fs::create_dir_all(project.join("Src")).expect("dependency source directory");
            let name = project.file_name().expect("project name").to_string_lossy();
            fs::write(
                project.join(format!("{name}.bproj")),
                format!("{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"{name}Lib\" {{\n  kind = \"Lib\"\n  entry = \"{name}.bd\"\n}}\n"),
            )
            .expect("dependency manifest");
            fs::write(project.join("Src").join(format!("{name}.bd")), "Fn Main() { }\n").expect("dependency source");
        }
        fs::write(app.join("Src/Main.bd"), "Fn Main() { }\n").expect("app source");
        let case = Self { lock: app.join("Project.lock"), root, app };
        case.write_app_manifest("Core");
        case
    }

    fn write_app_manifest(&self, dependency: &str) {
        fs::write(
            self.app.join("App.bproj"),
            format!(
                "App {{\n  name = \"App\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = \"App\"\n  entry = \"Main.bd\"\n}}\n\ndependency \"{dependency}\" {{\n  source = \"path\"\n  path = \"../{dependency}\"\n}}\n"
            ),
        )
        .expect("app manifest");
    }

    fn write_legacy_lock(&self) -> Vec<u8> {
        // Deliberately stale v1 source paths: migration must resolve the current manifest.
        let content = format!(
            "# Project.lock v1\nroot_manifest={}\nproject_name=App\ndependencies:\n- name=Core;manifest=/obsolete/Core/Core.bproj;project=/obsolete/Core;source_root=/obsolete/Core/Src;materialized_root=obj/beskid/deps/src/Core-old\n",
            self.app.join("App.bproj").display()
        );
        fs::write(&self.lock, content.as_bytes()).expect("legacy lock");
        content.into_bytes()
    }

    fn invoke(&self, command: &str, policy: Option<&str>) -> Output {
        let mut process = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
        process.arg(command).arg("--project").arg(&self.app).arg("--plain");
        if let Some(flag) = policy {
            process.arg(flag);
        }
        // The CLI may provision bundled Corelib before it resolves a project. Keep that
        // materialization inside this disposable fixture, including across repeated calls.
        process
            .env("BESKID_CORELIB_ROOT", self.root.join("installed-corelib"))
            .current_dir(&self.root)
            .output()
            .expect("run beskid command")
    }

    fn assert_no_obj(&self) {
        assert!(!self.app.join("obj").exists(), "rejected lock created obj/");
    }
}

impl Drop for ProjectCase {
    fn drop(&mut self) {
        // Remove only the temporary fixture created by this test.
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn output_text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn assert_failed_with(output: &Output, expected: &str) {
    let text = output_text(output);
    assert!(!output.status.success(), "command unexpectedly succeeded: {text}");
    assert!(text.contains(expected), "expected `{expected}` in diagnostic: {text}");
}

fn assert_invalid_lock_rejected_without_mutation(case: &ProjectCase, command: &str, original: &[u8]) {
    let output = case.invoke(command, None);
    let diagnostic = output_text(&output).to_ascii_lowercase();
    assert!(!output.status.success(), "{command} accepted an invalid lock: {diagnostic}");
    assert!(diagnostic.contains("lock"), "{command} failed without lockfile context: {diagnostic}");
    assert!(fs::read(&case.lock).expect("invalid lock retained") == original, "{command} rewrote the invalid lock");
    case.assert_no_obj();
}

fn assert_v2_lock(lock: &Path) -> String {
    let text = fs::read_to_string(lock).expect("v2 lock exists");
    assert!(text.starts_with("# Project.lock v2\n"), "wrong lock header: {text}");
    text
}

fn tree_snapshot(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn visit(root: &Path, current: &Path, entries: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        if !current.exists() {
            return;
        }
        let relative = current.strip_prefix(root).expect("snapshot path under root").to_path_buf();
        if current.is_dir() {
            entries.push((relative, None));
            let mut children = fs::read_dir(current)
                .expect("read snapshot directory")
                .map(|entry| entry.expect("snapshot entry").path())
                .collect::<Vec<_>>();
            children.sort();
            for child in children {
                visit(root, &child, entries);
            }
        } else {
            entries.push((relative, Some(fs::read(current).expect("read snapshot file"))));
        }
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries
}

#[test]
fn normal_build_run_and_test_reject_v1_with_migration_command() {
    for command in ["build", "run", "test"] {
        let case = ProjectCase::new();
        let legacy = case.write_legacy_lock();
        let output = case.invoke(command, None);
        assert_failed_with(&output, "beskid lock");
        assert_eq!(fs::read(&case.lock).expect("legacy lock retained"), legacy);
        case.assert_no_obj();
    }
}

#[test]
fn locked_and_frozen_reject_v1_without_writing_or_creating_obj() {
    for command in ["build", "run", "test"] {
        for policy in ["--locked", "--frozen"] {
            let case = ProjectCase::new();
            let legacy = case.write_legacy_lock();
            let output = case.invoke(command, Some(policy));
            assert_failed_with(&output, "beskid lock");
            assert_eq!(fs::read(&case.lock).expect("legacy lock retained"), legacy);
            case.assert_no_obj();
        }
    }
}

#[test]
fn lock_and_update_explicitly_migrate_v1_from_current_manifest() {
    for command in ["lock", "update"] {
        let case = ProjectCase::new();
        case.write_legacy_lock();
        let output = case.invoke(command, None);
        assert!(output.status.success(), "{command} failed: {}", output_text(&output));
        let lock = assert_v2_lock(&case.lock);
        assert!(lock.contains("name=Core"), "current dependency missing: {lock}");
        assert!(lock.contains("project=../Core"), "current relative project missing: {lock}");
        assert!(!lock.contains("/obsolete/"), "v1 path reused: {lock}");
    }
}

fn unknown_header_case(command: &str) {
    let case = ProjectCase::new();
    let unknown = b"# Project.lock v999\nroot_manifest=App.bproj\nproject_name=App\ndependencies:\n";
    fs::write(&case.lock, unknown).expect("write unknown-version lock");
    assert_invalid_lock_rejected_without_mutation(&case, command, unknown);
}

#[test]
fn unknown_lock_header_fails_for_build_without_preparation_writes() {
    unknown_header_case("build");
}

#[test]
fn unknown_lock_header_fails_for_lock_without_preparation_writes() {
    unknown_header_case("lock");
}

#[test]
fn unknown_lock_header_fails_for_update_without_preparation_writes() {
    unknown_header_case("update");
}

fn malformed_v2_case(command: &str) {
    let case = ProjectCase::new();
    let malformed = b"# Project.lock v2\nroot_manifest=App.bproj\nproject_name=App\ndependencies:\n- name=Core;source=path;project=../Core;manifest=Core.bproj;source_root=Src;materialized_root=obj/beskid/deps/src/Core-old;unexpected=1\n";
    fs::write(&case.lock, malformed).expect("write malformed v2 lock");
    assert_invalid_lock_rejected_without_mutation(&case, command, malformed);
}

#[test]
fn malformed_v2_lock_fails_for_build_without_preparation_writes() {
    malformed_v2_case("build");
}

#[test]
fn malformed_v2_lock_fails_for_lock_without_preparation_writes() {
    malformed_v2_case("lock");
}

#[test]
fn malformed_v2_lock_fails_for_update_without_preparation_writes() {
    malformed_v2_case("update");
}

#[test]
fn missing_lock_in_strict_modes_fails_before_creating_obj() {
    for policy in ["--locked", "--frozen"] {
        let case = ProjectCase::new();
        let output = case.invoke("build", Some(policy));
        assert_failed_with(&output, "Project.lock");
        assert!(!case.lock.exists(), "strict mode wrote a lock");
        case.assert_no_obj();
    }
}

#[test]
fn ordinary_build_creates_missing_v2_lock() {
    let case = ProjectCase::new();
    // A later compiler/runtime-kit failure is outside this policy check; preparation must
    // already have written the lock before the build can reach those stages.
    let _output = case.invoke("build", None);
    assert_v2_lock(&case.lock);
}

#[test]
fn stale_v2_lock_is_rejected_without_rewrite() {
    let case = ProjectCase::new();
    let lock_output = case.invoke("lock", None);
    assert!(lock_output.status.success(), "lock failed: {}", output_text(&lock_output));
    let original = fs::read(&case.lock).expect("read v2 lock");
    case.write_app_manifest("Other");
    let obj = case.app.join("obj");
    let original_obj = tree_snapshot(&obj);

    for policy in [None, Some("--locked"), Some("--frozen")] {
        let output = case.invoke("build", policy);
        let diagnostic = output_text(&output).to_ascii_lowercase();
        assert!(!output.status.success(), "stale graph unexpectedly accepted: {diagnostic}");
        assert!(diagnostic.contains("lock"), "missing lock context: {diagnostic}");
        assert!(
            diagnostic.contains("stale") || diagnostic.contains("out of date"),
            "missing stale-lock diagnosis: {diagnostic}"
        );
        assert!(diagnostic.contains("graph"), "missing graph-mismatch diagnosis: {diagnostic}");
        assert_eq!(fs::read(&case.lock).expect("v2 lock retained"), original);
        assert_eq!(tree_snapshot(&obj), original_obj, "stale lock mutated obj/");
    }
}
