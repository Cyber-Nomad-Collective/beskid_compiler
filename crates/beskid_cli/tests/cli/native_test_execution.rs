//! Public native test lifecycle controls; these require the candidate's exact native runtime kit.
use std::process::Command;

fn execute(source: &str, extra: &[&str]) -> std::process::Output {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("Main.bd");
    std::fs::write(&input, source).unwrap();
    Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["test", "--plain", "--json"])
        .arg(input)
        .args(extra)
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap()
}

#[test]
fn native_tests_isolate_assertion_failure_and_continue_selected_entries() {
    let output = execute(
        "use Testing.Assert;\ntest First { Assert.Fail(\"isolated failure\"); }\ntest Second { return; }\n",
        &[],
    );
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("First") && stdout.contains("Second"),
        "stdout: {stdout}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is one JSON record ({error}): {stdout}"));
    assert_eq!(record["schemaVersion"], 1);
    assert_eq!(record["command"], "test");
    assert_eq!(record["event"], "result");
    assert_eq!(record["status"], "failure");
    assert!(
        record["summary"]["passed"] == 1 && record["summary"]["failed"] == 1,
        "stdout: {stdout}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("JIT"));
}

fn outcomes(record: &serde_json::Value) -> Vec<(String, String)> {
    record["tests"]
        .as_array()
        .expect("tests array")
        .iter()
        .map(|test| (test["qualified_name"].as_str().unwrap().to_owned(), test["outcome"].as_str().unwrap().to_owned()))
        .collect()
}

/// A test inside `mod Topic { ... }` is discovered as `Topic::Name`, and the runner builds and
/// runs it by that qualified name: a passing nested test passes, a failing one fails, and the
/// same-named tests of two modules (and of the root) run as distinct tests with their own outcome.
#[test]
fn tests_in_inline_modules_run_by_qualified_name() {
    let output = execute(
        "use Testing.Assert;\n\
         mod Topic { test Inner { Assert.True(true, \"nested passes\"); } test Broken { Assert.Fail(\"nested fails\"); } }\n\
         mod Alpha { test Shared { return; } }\n\
         mod Beta { test Shared { Assert.Fail(\"beta shared fails\"); } mod Deep { test Shared { return; } } }\n\
         test Shared { return; }\n",
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "two tests fail: {stdout}; stderr: {stderr}");
    assert!(!stderr.contains("E4041"), "every qualified test is found: {stderr}");
    let record: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is one JSON record ({error}): {stdout}; stderr: {stderr}"));
    let pairs = |rows: &[(&str, &str)]| rows.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    assert_eq!(
        outcomes(&record),
        pairs(&[
            ("Topic::Inner", "passed"),
            ("Topic::Broken", "failed"),
            ("Alpha::Shared", "passed"),
            ("Beta::Shared", "failed"),
            ("Beta::Deep::Shared", "passed"),
            ("Shared", "passed"),
        ]),
        "{record}"
    );
    assert!(record["summary"]["passed"] == 4 && record["summary"]["failed"] == 2, "{record}");
    let beta = &record["tests"][3];
    assert_eq!(beta["name"], "Shared", "{record}");
    assert_eq!(beta["failure"]["kind"], "assertion", "{record}");
}

/// Only nested tests, all passing: the run succeeds.
#[test]
fn a_passing_nested_test_runs_by_qualified_name() {
    let output = execute("mod Topic { test Inner { return; } }\n", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "stdout: {stdout}; stderr: {stderr}");
    let record: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(outcomes(&record), vec![("Topic::Inner".to_owned(), "passed".to_owned())], "{record}");
}

/// Caller information end to end: `Assert.True(false)` omits `because` and the three caller
/// parameters; the compiler fills them at the call and the failure record read back at run time
/// names the line and column where `Assert.True` starts, not the test declaration.
#[test]
fn a_failed_assertion_reports_the_caller_information_filled_at_the_call() {
    // Runs on its own: the staged debug runtime kit stands in for an installed prefix.
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("Main.bd");
    std::fs::write(&input, "use Testing.Assert;\n\ntest CallerLine {\n      Assert.True(false);\n}\n").unwrap();
    let prefix = beskid_tests_mod_host::shared_runtime_prefix().expect("staged runtime kit");
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["test", "--plain", "--json"])
        .arg(&input)
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("BESKID_RUNTIME_PREFIX", &prefix)
        .env("BESKID_RUNTIME_KIT_PROFILE", "debug")
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stdout: {stdout}; stderr: {stderr}");
    let record: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is one JSON record ({error}): {stdout}; stderr: {stderr}"));
    let failure = &record["tests"][0]["failure"];
    assert_eq!(failure["kind"], "assertion", "{record}");
    assert!(failure.get("because").is_none(), "an omitted because adds nothing: {record}");
    let location = &failure["location"];
    assert!(location["file"].as_str().is_some_and(|file| file.ends_with("Main.bd")), "{record}");
    assert_eq!((location["line"].as_u64(), location["column"].as_u64()), (Some(4), Some(7)), "{record}");
}

#[test]
fn native_tests_budget_terminates_active_process() {
    // `Endless` never returns, so the command finishes only because the target budget stops it.
    let output = execute("test Endless { while (true) { } }\ntest Unstarted { return; }\n", &["--target-timeout", "2"]);
    assert!(!output.status.success());
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(text.contains("timed_out") || text.contains("budget expired"), "{text}");
}

#[test]
fn native_real_project_target_keeps_manifest_resolution() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Src")).unwrap();
    std::fs::write(
        root.path().join("Src/Main.bd"),
        "use Testing.Assert;\ntest ProjectSmoke { Assert.True(true, \"real project standard library\"); }\n",
    )
    .unwrap();
    let manifest = root.path().join("Native.bproj");
    std::fs::write(
        &manifest,
        "Native { name = \"Native\" version = \"0.1.0\" }\ntarget \"Smoke\" { kind = \"Test\" entry = \"Main.bd\" }\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["test", "--plain", "--json", "--project"])
        .arg(manifest)
        .args(["--target", "Smoke"])
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("ProjectSmoke"));
    assert!(!root.path().join("__synthetic__.bproj").exists());
}

#[cfg(unix)]
#[test]
fn native_matrix_parent_deadline_stops_active_nested_executable() {
    use std::process::Stdio;
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Src")).unwrap();
    std::fs::write(root.path().join("Src/Main.bd"), "test Endless { while (true) { } }\n").unwrap();
    let manifest = root.path().join("Matrix.bproj");
    std::fs::write(&manifest,
        "Matrix { name = \"Matrix\" version = \"0.1.0\" }\ntarget \"Endless\" { kind = \"Test\" entry = \"Main.bd\" }\n").unwrap();
    // Provision first: the whole-matrix deadline below bounds compilation and execution, never
    // the one-time build of a missing or stale dependency Mod artifact (the Corelib Serialization
    // Mod). `check` makes every dependency Mod of the project current. The deadline is generous so
    // the nested executable starts on a loaded host too; `Endless` never returns, so the matrix
    // ends only through that deadline, and the nested executable stays visible until it does.
    let provision = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["check", "--plain", "--project"])
        .arg(&manifest)
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(
        provision.status.success(),
        "provision dependency Mods before the timed matrix: {}{}",
        String::from_utf8_lossy(&provision.stdout),
        String::from_utf8_lossy(&provision.stderr)
    );
    let stdout_path = root.path().join("stdout");
    let stderr_path = root.path().join("stderr");
    let mut supervisor = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["test", "--plain", "--json", "--all-targets", "--project"])
        .arg(&manifest)
        .args(["--matrix-timeout", "60", "--target-timeout", "600"])
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .stdout(Stdio::from(std::fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let mut native_pid = None;
    let status = loop {
        let listing = Command::new("ps").args(["-ax", "-o", "pid=,ppid=,command="]).output().unwrap();
        let text = String::from_utf8_lossy(&listing.stdout);
        let rows: Vec<_> = text
            .lines()
            .filter_map(|line| {
                let mut fields = line.trim().splitn(3, char::is_whitespace);
                let pid = fields.next()?.parse::<u32>().ok()?;
                let rest = fields.collect::<Vec<_>>().join(" ");
                let rest = rest.trim_start();
                let (parent, command) = rest.split_once(char::is_whitespace)?;
                Some((pid, parent.parse::<u32>().ok()?, command.trim_start().to_owned()))
            })
            .collect();
        let worker = rows.iter().find(|(_, parent, _)| *parent == supervisor.id()).map(|(pid, _, _)| *pid);
        if let Some(worker) = worker {
            if let Some((pid, _, _)) =
                rows.iter().find(|(_, parent, command)| *parent == worker && command.ends_with("/entry-0/test"))
            {
                native_pid = Some(*pid);
            }
        }
        if let Some(status) = supervisor.try_wait().unwrap() {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = std::fs::read_to_string(stdout_path).unwrap();
    let stderr = std::fs::read_to_string(stderr_path).unwrap();
    assert!(!status.success(), "{stdout}{stderr}");
    let pid = native_pid.expect(&format!("fixture must observe actual nested native executable: {stdout}{stderr}"));
    let state = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output().unwrap();
    let state = String::from_utf8_lossy(&state.stdout).trim().to_owned();
    if !state.is_empty() && !state.starts_with('Z') {
        let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).status();
    }
    assert!(
        state.is_empty() || state.starts_with('Z'),
        "matrix cancellation left native process {pid} alive ({state}): {stdout}{stderr}"
    );
}
