//! `beskid check --all-members` checks every member of a `.bws` workspace in one process.
//!
//! The workspace is checked once with `--all-members` and once with one `beskid check` process
//! per member; both must agree, and the test reports both wall times.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

const MEMBERS: usize = 6;

fn check(root: &Path, corelib: &Path, args: &[&str]) -> (Output, Duration) {
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .arg("check")
        .args(args)
        .arg("--plain")
        .env("BESKID_CORELIB_ROOT", corelib)
        .current_dir(root)
        .output()
        .expect("run beskid check");
    (output, started.elapsed())
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn write_workspace(root: &Path, broken_member: Option<usize>) {
    let mut workspace = String::from("workspace {\n  name = \"many\"\n  resolver = v1\n}\n");
    for index in 0..MEMBERS {
        let name = format!("M{index}");
        workspace.push_str(&format!("\nmember \"{name}\" {{\n  path = \"{name}\"\n}}\n"));
        let dir = root.join(&name);
        fs::create_dir_all(dir.join("Src")).expect("member source directory");
        fs::write(
            dir.join(format!("{name}.bproj")),
            format!("{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = App\n  entry = \"Main.bd\"\n}}\n"),
        )
        .expect("member manifest");
        let body =
            if broken_member == Some(index) { "Output.WriteLineMissing(\"ok\");" } else { "Output.WriteLine(\"ok\");" };
        fs::write(
            dir.join("Src/Main.bd"),
            format!("use Core.Output;\n\ni32 Main() {{\n    {body}\n    return 0;\n}}\n"),
        )
        .expect("member source");
    }
    fs::write(root.join("Many.bws"), workspace).expect("workspace manifest");
}

/// Both modes start from their own fresh copy of the workspace (cold: no lock, no `obj/`), then
/// run again (warm), so neither reuses the other's caches.
#[test]
fn all_members_checks_every_member_in_one_process() {
    let corelib = tempfile::tempdir().expect("Corelib directory");
    let one = tempfile::tempdir().expect("one-process workspace");
    let many = tempfile::tempdir().expect("per-process workspace");
    // Both workspaces use one provisioned Corelib, so neither pays for provisioning.
    let corelib = corelib.path().join("corelib");
    for root in [one.path(), many.path()] {
        write_workspace(root, None);
    }
    let (provision, _) = check(one.path(), &corelib, &["--project", "M0", "--target", "App"]);
    assert!(provision.status.success(), "provisioning check: {}", text(&provision));
    fs::remove_dir_all(one.path().join("M0/obj")).expect("reset M0");
    fs::remove_file(one.path().join("M0/Project.lock")).expect("reset M0 lock");

    let mut one_process = Vec::new();
    for _ in 0..2 {
        let (all, elapsed) = check(one.path(), &corelib, &["--project", "Many.bws", "--all-members"]);
        assert!(all.status.success(), "workspace check: {}", text(&all));
        one_process.push(elapsed);
    }
    for index in 0..MEMBERS {
        assert!(one.path().join(format!("M{index}/Project.lock")).is_file(), "member M{index} was not checked");
    }

    let mut per_process = Vec::new();
    for _ in 0..2 {
        let mut total = Duration::ZERO;
        for index in 0..MEMBERS {
            let (output, elapsed) = check(many.path(), &corelib, &["--project", &format!("M{index}")]);
            assert!(output.status.success(), "member M{index}: {}", text(&output));
            total += elapsed;
        }
        per_process.push(total);
    }
    eprintln!(
        "check {MEMBERS} workspace members: one process (--all-members) cold {:?} warm {:?}; \
         one process per member cold {:?} warm {:?}",
        one_process[0], one_process[1], per_process[0], per_process[1]
    );
}

fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            if entry.file_type().expect("file type").is_dir() {
                directory_bytes(&entry.path())
            } else {
                entry.metadata().map_or(0, |metadata| metadata.len())
            }
        })
        .sum()
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create copy directory");
    for entry in fs::read_dir(from).expect("read directory") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Before and after 0.6.7: the same workspace checked with the sealed shared Corelib (read in
/// place) and with an unsealed copy of that Corelib, which is copied into every project like every
/// Corelib was before. Reports `obj/beskid/deps` size and the cold and warm check times.
#[test]
fn shared_corelib_saves_the_per_project_copies() {
    let corelib = tempfile::tempdir().expect("Corelib directory");
    let sealed = corelib.path().join("sealed");
    let shared = tempfile::tempdir().expect("shared workspace");
    let copied = tempfile::tempdir().expect("copying workspace");
    write_workspace(shared.path(), None);
    write_workspace(copied.path(), None);
    let measure = |root: &Path, corelib: &Path| {
        let mut times = Vec::new();
        for _ in 0..2 {
            let (output, elapsed) = check(root, corelib, &["--project", "Many.bws", "--all-members"]);
            assert!(output.status.success(), "workspace check: {}", text(&output));
            times.push(elapsed);
        }
        let bytes =
            (0..MEMBERS).map(|index| directory_bytes(&root.join(format!("M{index}/obj/beskid/deps")))).sum::<u64>();
        (times, bytes)
    };
    let (after_times, after_bytes) = measure(shared.path(), &sealed);
    let unsealed = corelib.path().join("unsealed");
    copy_tree(&sealed, &unsealed);
    // The bundle fingerprint marker (`beskid_abi::corelib_bundle::CORELIB_BUNDLE_FINGERPRINT_FILE`).
    fs::remove_file(unsealed.join(".beskid-bundle.sha256")).expect("unseal the copy");
    let (before_times, before_bytes) = measure(copied.path(), &unsealed);
    eprintln!(
        "{MEMBERS} members: per-project Corelib copies {before_bytes} bytes, cold {:?} warm {:?}; \
         shared Corelib {after_bytes} bytes, cold {:?} warm {:?}",
        before_times[0], before_times[1], after_times[0], after_times[1]
    );
    assert_eq!(after_bytes, 0, "the shared Corelib is never copied");
    assert!(before_bytes > 0, "the unsealed baseline copies Corelib into every project");
}

#[test]
fn all_members_reports_a_failing_member_and_checks_the_rest() {
    let root = tempfile::tempdir().expect("fixture directory");
    write_workspace(root.path(), Some(2));
    let (output, _) = check(
        root.path(),
        &root.path().join("corelib"),
        &["--project", root.path().to_str().unwrap(), "--all-members"],
    );
    let report = text(&output);
    assert!(!output.status.success(), "a broken member fails the workspace check: {report}");
    assert!(report.contains("WriteLineMissing"), "{report}");
    assert!(report.contains("1 of 6 workspace members failed") && report.contains("member `M2`"), "{report}");
    assert!(root.path().join("M5/Project.lock").is_file(), "members after the broken one are still checked");

    let (rejected, _) = check(root.path(), &root.path().join("corelib"), &["--project", "M0", "--all-members"]);
    assert!(!rejected.status.success());
    assert!(text(&rejected).contains("`.bws` workspace"), "{}", text(&rejected));
}

/// A parse error found while a member's active Mods are decided (a detached `///` header on line 1)
/// is reported with its code at the authored file, exactly as a single-project check reports it,
/// never as an uncoded failure line naming the member's `obj/beskid/root` copy.
#[test]
fn all_members_reports_a_member_parse_error_with_its_code_at_the_authored_file() {
    let root = tempfile::tempdir().expect("fixture directory");
    write_workspace(root.path(), None);
    fs::write(
        root.path().join("M3/Src/Main.bd"),
        "/// The member's entry point.\n\nuse Core.Output;\n\ni32 Main() {\n    Output.WriteLine(\"ok\");\n    return 0;\n}\n",
    )
    .expect("member source with a detached header");
    let corelib = root.path().join("corelib");
    let (single, _) = check(root.path(), &corelib, &["--project", "M3"]);
    let (output, _) = check(root.path(), &corelib, &["--project", "Many.bws", "--all-members"]);
    for (mode, output) in [("single", &single), ("all-members", &output)] {
        let report = text(output);
        assert!(!output.status.success(), "{mode}: the detached header fails the check: {report}");
        assert!(report.contains("E0117"), "{mode}: the parse error keeps its code: {report}");
        assert!(report.contains("M3/Src/Main.bd"), "{mode}: the error names the authored file: {report}");
        assert!(!report.contains("obj/beskid"), "{mode}: the error never names the materialized copy: {report}");
        assert!(!report.contains("to decide its active Mods"), "{mode}: no flattened activation context: {report}");
    }
    assert!(text(&output).contains("1 of 6 workspace members failed"), "{}", text(&output));
}
