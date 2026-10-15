//! CLI06-03/04: `--quiet` / `--verbose` output policy, `check --json` records and `run` cancellation.
//! Runs against the provisioned Corelib and runtime kit of the test environment.
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn beskid(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
    command
        .env("BESKID_CONFIG_DIR", root.join("config"))
        .env("BESKID_HOME", root.join("toolchain-home"))
        .env("OTEL_SDK_DISABLED", "true")
        .env_remove("RUST_LOG")
        .stdin(Stdio::null());
    command
}

fn text(output: &Output) -> (String, String) {
    (String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned())
}

fn source(root: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = root.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn v06_quiet_suppresses_progress_and_verbose_keeps_it() {
    let root = tempfile::tempdir().unwrap();
    let valid = source(root.path(), "Valid.bd", "pub i32 Main() { return 0; }\n");
    let normal = beskid(root.path()).args(["check", "--plain"]).arg(&valid).output().unwrap();
    let (_, normal_err) = text(&normal);
    assert!(normal.status.success(), "{normal_err}");
    assert!(normal_err.contains("Analyze complete"), "normal output keeps the summary: {normal_err}");

    let quiet = beskid(root.path()).args(["--quiet", "check", "--plain"]).arg(&valid).output().unwrap();
    let (_, quiet_err) = text(&quiet);
    assert!(quiet.status.success(), "{quiet_err}");
    assert!(!quiet_err.contains("Analyze complete") && !quiet_err.contains(" INFO "), "quiet output: {quiet_err}");

    let invalid = source(root.path(), "Invalid.bd", "pub i32 Main() { return UnknownValue; }\n");
    let failed = beskid(root.path()).args(["check", "-q", "--plain"]).arg(&invalid).output().unwrap();
    let (_, failed_err) = text(&failed);
    assert_eq!(failed.status.code(), Some(1), "{failed_err}");
    assert!(failed_err.contains("unknown value `UnknownValue`"), "quiet keeps diagnostics: {failed_err}");

    let verbose = beskid(root.path()).args(["check", "--verbose", "--plain"]).arg(&valid).output().unwrap();
    let (_, verbose_err) = text(&verbose);
    assert!(verbose.status.success() && verbose_err.contains("Analyze complete"), "{verbose_err}");

    let both = beskid(root.path()).args(["--quiet", "--verbose", "check"]).arg(&valid).output().unwrap();
    assert_eq!(both.status.code(), Some(2), "--quiet conflicts with --verbose");
}

#[test]
fn v06_check_json_emits_diagnostic_and_result_records() {
    let root = tempfile::tempdir().unwrap();
    let invalid = source(root.path(), "Invalid.bd", "pub i32 Main() { return UnknownValue; }\n");
    let output = beskid(root.path()).args(["check", "--json", "--plain"]).arg(&invalid).output().unwrap();
    let (stdout, stderr) = text(&output);
    assert_eq!(output.status.code(), Some(1), "{stdout}{stderr}");
    let records: Vec<serde_json::Value> =
        stdout.lines().map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{error}: {line}"))).collect();
    assert!(records.iter().all(|record| record["schemaVersion"] == 1 && record["command"] == "check"), "{stdout}");
    let diagnostic = records.iter().find(|record| record["event"] == "diagnostic").expect("diagnostic record");
    assert_eq!(diagnostic["status"], "error");
    assert!(diagnostic["diagnostic"]["message"].as_str().unwrap().contains("UnknownValue"), "{diagnostic}");
    assert_eq!(diagnostic["diagnostic"]["span"]["start"]["line"], 1, "{diagnostic}");
    let last = records.last().unwrap();
    assert_eq!((last["event"].as_str(), last["status"].as_str()), (Some("result"), Some("failure")), "{stdout}");
    assert_eq!(records.iter().filter(|record| record["event"] == "result").count(), 1);

    let valid = source(root.path(), "Valid.bd", "pub i32 Main() { return 0; }\n");
    let output = beskid(root.path()).args(["check", "--json", "--plain"]).arg(&valid).output().unwrap();
    let (stdout, stderr) = text(&output);
    assert!(output.status.success(), "{stdout}{stderr}");
    let result: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!((result["event"].as_str(), result["status"].as_str()), (Some("result"), Some("success")));
    assert_eq!(result["errors"], 0);
}

/// SIGINT sent to the CLI alone reaches the running program, the CLI survives to reap it, and the
/// CLI exits 130 (128 + SIGINT) because the program died of the signal.
#[cfg(unix)]
#[test]
fn v06_run_interrupt_reaches_program_and_exits_130() {
    use std::io::BufRead;
    use std::sync::mpsc;
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    let program = source(
        root.path(),
        "Wait.bd",
        "use Core.Output;\ni32 Main() {\n    Output.WriteLine(\"ready\");\n    while (true) { }\n    return 0;\n}\n",
    );
    let mut cli = beskid(root.path()).args(["run", "--plain"]).arg(&program).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let stdout = cli.stdout.take().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut lines = std::io::BufReader::new(stdout).lines();
        let mut seen = Vec::new();
        while let Some(Ok(line)) = lines.next() {
            if line == "ready" {
                let _ = ready_tx.send(());
            }
            seen.push(line);
        }
        seen
    });
    let stderr = cli.stderr.take().unwrap();
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut std::io::BufReader::new(stderr), &mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    });
    if ready_rx.recv_timeout(Duration::from_secs(600)).is_err() {
        let _ = cli.kill();
        let _ = cli.wait();
        panic!("program never started: {}", stderr_reader.join().unwrap());
    }
    let interrupted = Command::new("kill").args(["-INT", &cli.id().to_string()]).status().unwrap();
    assert!(interrupted.success());
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = cli.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = cli.kill();
            let _ = cli.wait();
            panic!("CLI did not finish after SIGINT: {}", stderr_reader.join().unwrap());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let lines = reader.join().unwrap();
    let stderr = stderr_reader.join().unwrap();
    assert_eq!(status.code(), Some(130), "the CLI reaps the interrupted program and exits 128+SIGINT: {stderr}");
    assert_eq!(lines, ["ready"], "stdout carries only program output");
    assert!(stderr.contains("exit: 130"), "the run summary reports the program status: {stderr}");
}
