//! End-to-end policy checks at the `beskid` command boundary.
//!
//! These fixtures intentionally use only local path dependencies. A rejected lock must fail
//! during project preparation, before build, run, or test can reach code generation.

use std::fs::{self, File, FileTimes};
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
        let core = root.join("Kernel");
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
        case.write_app_manifest("Kernel");
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

    /// A lock whose first line is not the format 3 header.
    fn write_unsupported_lock(&self, header: &str) -> Vec<u8> {
        let content = format!(
            "{header}\nroot_manifest=App.bproj\nproject_name=App\ndependencies:\n- name=Kernel;source=path;project=../Kernel;manifest=Kernel.bproj;source_root=Src;materialized_root=obj/beskid/deps/src/Kernel\n"
        );
        fs::write(&self.lock, content.as_bytes()).expect("unsupported lock");
        content.into_bytes()
    }

    fn invoke(&self, command: &str, policy: Option<&str>) -> Output {
        let mut process = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
        match command {
            // `beskid lock` moved under `dev project`; `update` has no progress flag and needs an explicit scope.
            "lock" => process.args(["dev", "project", "lock"]).arg("--project").arg(&self.app).arg("--plain"),
            "update" => process.args(["update", "--all"]).arg("--project").arg(&self.app),
            other => process.arg(other).arg("--project").arg(&self.app).arg("--plain"),
        };
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

fn assert_current_lock(lock: &Path) -> String {
    let text = fs::read_to_string(lock).expect("lock exists");
    assert!(text.starts_with("# Project.lock v3\n"), "wrong lock header: {text}");
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

/// An older lock format or an unknown header fails closed (E3025) in every command, the explicit
/// `dev project lock` and `update --all` included: the message says to delete the file, the lock
/// bytes stay and nothing is prepared.
#[test]
fn unsupported_lock_format_fails_closed_for_every_command() {
    let mut runs = Vec::new();
    for header in ["# Project.lock v1", "# Project.lock v2", "# Project.lock v999"] {
        runs.extend([(header, "build", None), (header, "lock", None), (header, "update", None)]);
    }
    runs.extend([
        ("# Project.lock v1", "run", None),
        ("# Project.lock v1", "test", None),
        ("# Project.lock v1", "build", Some("--locked")),
        ("# Project.lock v1", "build", Some("--frozen")),
    ]);
    for (header, command, policy) in runs {
        let case = ProjectCase::new();
        let original = case.write_unsupported_lock(header);
        let output = case.invoke(command, policy);
        let text = output_text(&output);
        assert!(!output.status.success(), "{header} {command} {policy:?} accepted the lock: {text}");
        assert!(
            text.contains("unsupported format")
                && text.contains("delete the file")
                && text.contains("beskid dev project lock"),
            "{header} {command} {policy:?}: {text}"
        );
        assert_eq!(fs::read(&case.lock).expect("lock retained"), original, "{header} {command} {policy:?}");
        case.assert_no_obj();
    }
}

#[test]
fn removed_root_lock_command_names_its_dev_project_replacement() {
    let case = ProjectCase::new();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["lock", "--project"])
        .arg(&case.app)
        .env("BESKID_CORELIB_ROOT", case.root.join("installed-corelib"))
        .current_dir(&case.root)
        .output()
        .expect("run removed lock command");
    assert_eq!(output.status.code(), Some(2), "usage status expected: {}", output_text(&output));
    let text = output_text(&output);
    assert!(text.contains("`beskid lock` was removed") && text.contains("beskid dev project lock"), "{text}");
    assert!(!case.lock.exists(), "removed command wrote a lock");
    case.assert_no_obj();
}

fn malformed_lock_case(command: &str) {
    let case = ProjectCase::new();
    let malformed = b"# Project.lock v3\nroot_manifest=App.bproj\nproject_name=App\ndependencies:\n- name=Kernel;source=path;project=../Kernel;manifest=Kernel.bproj;source_root=Src;materialized_root=obj/beskid/deps/src/Kernel-old;unexpected=1\n";
    fs::write(&case.lock, malformed).expect("write malformed lock");
    assert_invalid_lock_rejected_without_mutation(&case, command, malformed);
}

#[test]
fn malformed_lock_fails_for_build_without_preparation_writes() {
    malformed_lock_case("build");
}

#[test]
fn malformed_lock_fails_for_lock_without_preparation_writes() {
    malformed_lock_case("lock");
}

#[test]
fn malformed_lock_fails_for_update_without_preparation_writes() {
    malformed_lock_case("update");
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
fn ordinary_build_creates_missing_lock() {
    let case = ProjectCase::new();
    // A later compiler/runtime-kit failure is outside this policy check; preparation must
    // already have written the lock before the build can reach those stages.
    let _output = case.invoke("build", None);
    assert_current_lock(&case.lock);
}

#[cfg(unix)]
#[test]
fn lock_rejects_symlink_to_external_file_without_reading_or_rewriting_it() {
    use std::os::unix::fs::symlink;

    let case = ProjectCase::new();
    let external = case.root.join("external.lock");
    let original = case.write_unsupported_lock("# Project.lock v1");
    fs::rename(&case.lock, &external).expect("move the lock outside project");
    symlink(&external, &case.lock).expect("symlink lockfile");

    let output = case.invoke("lock", None);
    assert_failed_with(&output, "Project.lock");
    assert_eq!(fs::read(&external).expect("external file retained"), original);
    assert!(fs::symlink_metadata(&case.lock).expect("lock link retained").file_type().is_symlink());
    case.assert_no_obj();
}

#[cfg(unix)]
#[test]
fn lock_rejects_dangling_symlink_without_creating_external_file() {
    use std::os::unix::fs::symlink;

    let case = ProjectCase::new();
    let external = case.root.join("missing-external.lock");
    symlink(&external, &case.lock).expect("dangling lockfile link");

    let output = case.invoke("lock", None);
    assert_failed_with(&output, "Project.lock");
    assert!(!external.exists(), "lock wrote through dangling symlink");
    assert!(fs::symlink_metadata(&case.lock).expect("lock link retained").file_type().is_symlink());
    case.assert_no_obj();
}

#[test]
fn stale_lock_is_rejected_without_rewrite() {
    let case = ProjectCase::new();
    let lock_output = case.invoke("lock", None);
    assert!(lock_output.status.success(), "lock failed: {}", output_text(&lock_output));
    let original = fs::read(&case.lock).expect("read lock");
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
        assert_eq!(fs::read(&case.lock).expect("lock retained"), original);
        assert_eq!(tree_snapshot(&obj), original_obj, "stale lock mutated obj/");
    }
}

#[test]
fn copied_source_project_has_identical_lock_and_locked_commands_preserve_it() {
    let original = ProjectCase::new();
    let relocated = ProjectCase::new();
    fs::write(original.app.join("Src/Main.bd"), "unit Main() {}\n").expect("app source");
    fs::write(original.root.join("Kernel/Src/Kernel.bd"), "unit Kernel() {}\n").expect("dependency source");
    fs::write(original.app.join("Src/Tests.bd"), "test Smoke {}\n").expect("test source");
    let mut manifest = fs::read_to_string(original.app.join("App.bproj")).expect("app manifest");
    manifest.push_str("\ntarget \"AppTests\" {\n  kind = \"Test\"\n  entry = \"Tests.bd\"\n}\n");
    fs::write(original.app.join("App.bproj"), manifest).expect("test target manifest");
    for relative in
        ["App/App.bproj", "App/Src/Main.bd", "App/Src/Tests.bd", "Kernel/Kernel.bproj", "Kernel/Src/Kernel.bd"]
    {
        fs::copy(original.root.join(relative), relocated.root.join(relative)).expect("copy source-bearing fixture");
    }

    let first_lock_output = original.invoke("lock", None);
    assert!(first_lock_output.status.success(), "first lock failed: {}", output_text(&first_lock_output));
    let second_lock_output = relocated.invoke("lock", None);
    assert!(second_lock_output.status.success(), "relocated lock failed: {}", output_text(&second_lock_output));
    let expected = fs::read(&original.lock).expect("first generated lock");
    assert!(expected.starts_with(b"# Project.lock v3\n"), "expected portable v3 lock");
    assert!(String::from_utf8_lossy(&expected).contains("source=path"), "fixture must exercise its path dependency");
    assert_eq!(fs::read(&relocated.lock).expect("relocated generated lock"), expected);

    // A fixed timestamp makes a lock rewrite observable even on coarse-grained filesystems.
    let original_mtime = UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
    File::options()
        .write(true)
        .open(&relocated.lock)
        .expect("open relocated lock")
        .set_times(FileTimes::new().set_modified(original_mtime))
        .expect("set lock timestamp sentinel");
    let expected_mtime = fs::metadata(&relocated.lock).expect("lock metadata").modified().expect("lock mtime");

    for command in ["build", "run", "test"] {
        let obj = relocated.app.join("obj");
        fs::remove_dir_all(&obj).expect("remove only disposable prepared workspace");
        let output = relocated.invoke(command, Some("--locked"));
        let diagnostic = output_text(&output);
        assert!(obj.exists(), "{command} did not prepare the copied project: {diagnostic}");
        assert!(
            !diagnostic.contains("Project.lock"),
            "{command} rejected a lock generated from the copied sources: {diagnostic}"
        );
        assert_eq!(fs::read(&relocated.lock).expect("lock retained"), expected, "{command} changed lock bytes");
        assert_eq!(
            fs::metadata(&relocated.lock).expect("lock metadata").modified().expect("lock mtime"),
            expected_mtime,
            "{command} rewrote Project.lock"
        );
    }
}
