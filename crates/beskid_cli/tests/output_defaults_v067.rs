//! Default output, `--plain`, `-q`, `-v`/`-vv`, log destinations and early-closed stdout (v0.6.7).
//!
//! Each case runs the real binary against a clean one-file input. The assertions read stderr
//! only: default output carries no info logs and no pipeline tree, `-v` restores both, `-q` prints
//! no status line, and `--log-file` / `BESKID_LOG` move logs off stderr.

use std::fs;
use std::process::{Command, Output, Stdio};

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("fixture directory");
        fs::write(root.path().join("Valid.bd"), "pub i32 Main() { return 0; }\n").expect("write source");
        Self { root }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
        command
            .env("BESKID_CORELIB_ROOT", self.root.path().join("corelib"))
            .env_remove("RUST_LOG")
            .env_remove("BESKID_LOG")
            .env_remove("NO_COLOR")
            .stdin(Stdio::null());
        command
    }

    fn check(&self, flags: &[&str]) -> Output {
        let output = self
            .command()
            .args(flags)
            .arg("check")
            .arg("--plain")
            .arg(self.root.path().join("Valid.bd"))
            .output()
            .expect("run beskid check");
        assert!(output.status.success(), "{flags:?}: {}", String::from_utf8_lossy(&output.stderr));
        output
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn default_output_has_no_info_logs_and_no_pipeline_tree() {
    let text = stderr(&Fixture::new().check(&[]));
    for forbidden in ["INFO", "tracing subscriber initialized", "Resolve manifest", "Materialize dependencies", "Salsa", "No diagnostics."] {
        assert!(!text.contains(forbidden), "default output leaked `{forbidden}`:\n{text}");
    }
    assert!(text.lines().count() <= 1, "default output is at most one status line:\n{text}");
}

#[test]
fn plain_keeps_default_verbosity_and_prints_no_escape_sequences() {
    let fixture = Fixture::new();
    let plain = stderr(&fixture.check(&[]));
    assert!(!plain.contains('\u{1b}'), "--plain output carries escape sequences: {plain:?}");
    let verbose = stderr(&fixture.check(&["-v"]));
    assert!(!verbose.contains('\u{1b}'), "--plain -v output carries escape sequences: {verbose:?}");
    assert!(verbose.lines().count() > plain.lines().count(), "-v adds detail:\n{verbose}");
}

#[test]
fn verbose_shows_info_logs_and_the_pipeline_tree() {
    let text = stderr(&Fixture::new().check(&["-v"]));
    assert!(text.contains("tracing subscriber initialized"), "-v shows info logs:\n{text}");
    assert!(text.contains("Resolve manifest"), "-v shows pipeline phases:\n{text}");
}

#[test]
fn double_verbose_keeps_the_verbose_output() {
    let text = stderr(&Fixture::new().check(&["-vv"]));
    assert!(text.contains("tracing subscriber initialized") && text.contains("Resolve manifest"), "{text}");
}

#[test]
fn quiet_prints_no_status_line_and_no_info_logs() {
    let text = stderr(&Fixture::new().check(&["-q"]));
    for forbidden in ["INFO", "complete", "Resolve manifest", "No diagnostics."] {
        assert!(!text.contains(forbidden), "-q leaked `{forbidden}`:\n{text}");
    }
}

#[test]
fn quiet_and_verbose_together_are_a_usage_error() {
    let fixture = Fixture::new();
    let output = fixture.command().args(["-q", "-v", "check"]).arg(fixture.root.path().join("Valid.bd")).output().expect("run");
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn log_file_flag_moves_logs_off_stderr() {
    let fixture = Fixture::new();
    let log = fixture.root.path().join("logs").join("beskid.log");
    let output = fixture.check(&["-v", "--log-file", log.to_str().expect("utf-8 path")]);
    let text = fs::read_to_string(&log).expect("log file written");
    assert!(text.contains("tracing subscriber initialized"), "{text}");
    assert!(!stderr(&output).contains("tracing subscriber initialized"), "log line stayed on stderr");
}

#[test]
fn beskid_log_environment_variable_names_the_log_file() {
    let fixture = Fixture::new();
    let log = fixture.root.path().join("env.log");
    let output = fixture
        .command()
        .env("BESKID_LOG", &log)
        .args(["-v", "check", "--plain"])
        .arg(fixture.root.path().join("Valid.bd"))
        .output()
        .expect("run beskid check");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::read_to_string(&log).expect("log file written").contains("tracing subscriber initialized"));
    assert!(!stderr(&output).contains("tracing subscriber initialized"));
}

/// `beskid ... | head` closes stdout early: the process ends quietly (SIGPIPE status 141 or exit 1),
/// never SIGABRT (134) and never a panic message.
#[cfg(unix)]
#[test]
fn closed_stdout_ends_quietly_without_abort_or_panic() {
    use std::os::unix::process::ExitStatusExt;

    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .args(["dev", "capabilities"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn beskid");
    drop(child.stdout.take());
    let output = child.wait_with_output().expect("wait");
    let text = stderr(&output);
    assert!(!text.contains("panicked") && !text.contains("failed printing"), "panic message on EPIPE: {text}");
    assert_ne!(output.status.signal(), Some(libc::SIGABRT), "aborted: {text}");
    assert_ne!(output.status.code(), Some(134), "abort status: {text}");
    let quiet_end = output.status.signal() == Some(libc::SIGPIPE) || matches!(output.status.code(), Some(0 | 1 | 141));
    assert!(quiet_end, "unexpected status {:?}: {text}", output.status);
}
