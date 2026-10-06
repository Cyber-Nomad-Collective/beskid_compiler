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
fn v06_native_tests_isolate_assertion_failure_and_continue_selected_entries() {
    let output = execute(
        "use Std.Testing.Assert;\ntest First { Assert.Fail(\"isolated failure\"); }\ntest Second { return; }\n",
        &[],
    );
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("First") && stdout.contains("Second"),
        "stdout: {stdout}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("\"passed\": 1") && stdout.contains("\"failed\": 1"),
        "stdout: {stdout}; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("JIT"));
}

#[test]
fn v06_native_tests_budget_terminates_active_process() {
    let started = std::time::Instant::now();
    let output = execute("test Endless { while (true) { } }\ntest Unstarted { return; }\n", &["--target-timeout", "2"]);
    assert!(!output.status.success());
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "native child exceeded bounded termination window");
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(text.contains("timed_out") || text.contains("budget expired"), "{text}");
}

#[test]
fn v06_native_real_project_target_keeps_manifest_resolution() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Src")).unwrap();
    std::fs::write(
        root.path().join("Src/Main.bd"),
        "use Std.Testing.Assert;\ntest ProjectSmoke { Assert.True(true, \"real project standard library\"); }\n",
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

#[test]
fn v06_native_external_unprefixed_internal_module_is_not_public_import_authority() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("Main.bd");
    std::fs::write(&input, "use Testing.Assert;\ntest Unprefixed { Assert.Fail(\"must never execute\"); }\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["test", "--plain", "--json"])
        .arg(&input)
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(text.contains("Testing.Assert") && (text.contains("unknown") || text.contains("resolve")), "{text}");
    assert!(
        !text.contains("native test `Unprefixed` exited"),
        "invalid public import must fail before native execution: {text}"
    );
    assert!(!root.path().join("__synthetic__.bproj").exists());
}

#[cfg(unix)]
#[test]
fn v06_native_matrix_parent_deadline_stops_active_nested_executable() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("Src")).unwrap();
    std::fs::write(root.path().join("Src/Main.bd"), "test Endless { while (true) { } }\n").unwrap();
    let manifest = root.path().join("Matrix.bproj");
    std::fs::write(&manifest,
        "Matrix { name = \"Matrix\" version = \"0.1.0\" }\ntarget \"Endless\" { kind = \"Test\" entry = \"Main.bd\" }\n").unwrap();
    let stdout_path = root.path().join("stdout");
    let stderr_path = root.path().join("stderr");
    let mut supervisor = Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
        .args(["test", "--plain", "--json", "--all-targets", "--project"])
        .arg(&manifest)
        .args(["--matrix-timeout", "5", "--target-timeout", "30"])
        .env("BESKID_CONFIG_DIR", root.path().join("config"))
        .env("BESKID_HOME", root.path().join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .stdout(Stdio::from(std::fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let started = Instant::now();
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
        if started.elapsed() > Duration::from_secs(30) {
            let _ = supervisor.kill();
            let _ = supervisor.wait();
            panic!("matrix parent did not terminate within bounded fixture window");
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
