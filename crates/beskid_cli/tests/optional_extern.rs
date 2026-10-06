//! End-to-end `[Extern(..., Optional:true)]` contracts through the CLI.
//!
//! An optional contract that names a library which is not installed loads under the JIT
//! (`beskid test`) and as an AOT executable; its `Available()` query is false and the pure path
//! runs. The same contract over libc is available and its calls work. Calling an absent symbol
//! raises the ABI-v5 `extern_unavailable` trap (code 11, exit status 101). A non-optional
//! contract over a missing library still fails to load.
//!
//! The JIT and AOT cases need an installed exact runtime kit with debug and release profiles;
//! they run when `BESKID_RUNTIME_PREFIX` names one and are skipped otherwise. The fixtures name
//! `libc.so.6`, so the runtime cases are Linux-only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const FIXTURE: &str = include_str!("fixtures/optional_extern/Main.bd");
const TRAP_FIXTURE: &str = include_str!("fixtures/optional_extern/Trap.bd");
const REQUIRED_FIXTURE: &str = include_str!("fixtures/optional_extern/Required.bd");

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(label: &str, source: &str, with_corelib: bool) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid_optional_{label}_{}_{nonce}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("project source directory");
        let corelib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/beskid_corelib");
        let dependency = if with_corelib {
            format!(
                "dependency \"corelib\" {{\n  source = \"path\"\n  path = \"{}\"\n}}\n",
                corelib.canonicalize().expect("in-tree Corelib").display()
            )
        } else {
            String::new()
        };
        fs::write(
            root.join("OptionalExtern.bproj"),
            format!(
                "OptionalExtern {{\n  name = \"OptionalExtern\"\n  version = \"0.1.0\"\n  root = \"src\"\n}}\n\n\
                 {dependency}\ntarget \"App\" {{\n  kind = App\n  entry = \"Main.bd\"\n}}\n\ntarget \"Tests\" {{\n  \
                 kind = Lib\n  entry = \"Main.bd\"\n}}\n"
            ),
        )
        .expect("project manifest");
        fs::write(root.join("src/Main.bd"), source).expect("project source");
        Self { root }
    }

    fn beskid(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
            .args(arguments)
            .arg("--plain")
            .arg("--project")
            .arg(&self.root)
            .env_remove("RUST_LOG")
            .env("BESKID_CORELIB_ROOT", self.root.join("installed-corelib"))
            .current_dir(&self.root)
            .output()
            .expect("run beskid")
    }

    fn build(&self, name: &str) -> (Output, PathBuf) {
        let executable = self.root.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let output = self.beskid(&["build", "--target", "App", "--output", executable.to_str().expect("UTF-8 path")]);
        (output, executable)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[cfg(target_os = "linux")]
fn runtime_kit_available() -> bool {
    let available = std::env::var_os("BESKID_RUNTIME_PREFIX").is_some_and(|prefix| !prefix.is_empty());
    if !available {
        eprintln!("skipping: BESKID_RUNTIME_PREFIX does not name an installed exact runtime kit");
    }
    available
}

#[cfg(target_os = "linux")]
const TRAP_DIAGNOSTIC: &str = "beskid runtime trap v5: extern_unavailable (11)";

#[cfg(target_os = "linux")]
#[test]
fn optional_extern_loads_and_selects_path_under_jit() {
    if !runtime_kit_available() {
        return;
    }
    let project = Project::new("jit", FIXTURE, true);
    let output = project.beskid(&["test", "--target", "Tests"]);
    let log = text(&output);
    assert!(output.status.success(), "beskid test failed:\n{log}");
    assert!(log.contains("Result: passed=4, failed=0"), "unexpected test summary:\n{log}");
}

#[cfg(target_os = "linux")]
#[test]
fn optional_extern_loads_and_selects_path_after_aot_build() {
    if !runtime_kit_available() {
        return;
    }
    let project = Project::new("aot", FIXTURE, true);
    let (output, executable) = project.build("optional_app");
    assert!(output.status.success(), "beskid build failed:\n{}", text(&output));
    let run = Command::new(&executable).output().expect("run AOT executable");
    assert_eq!(run.status.code(), Some(0), "AOT optional extern checks failed (exit code names the check):\n{}", text(&run));
}

#[cfg(target_os = "linux")]
#[test]
fn calling_an_absent_optional_symbol_traps_under_jit_and_aot() {
    if !runtime_kit_available() {
        return;
    }
    let project = Project::new("trap", TRAP_FIXTURE, true);
    let output = project.beskid(&["test", "--target", "Tests"]);
    let log = text(&output);
    assert!(!output.status.success(), "the call must trap:\n{log}");
    assert!(log.contains(TRAP_DIAGNOSTIC), "missing extern_unavailable trap:\n{log}");

    let (output, executable) = project.build("optional_trap");
    assert!(output.status.success(), "beskid build failed:\n{}", text(&output));
    let run = Command::new(&executable).output().expect("run AOT executable");
    let log = text(&run);
    assert_eq!(run.status.code(), Some(101), "the AOT call must trap:\n{log}");
    assert!(log.contains(TRAP_DIAGNOSTIC), "missing extern_unavailable trap:\n{log}");
}

#[cfg(target_os = "linux")]
#[test]
fn required_extern_with_missing_library_still_fails_to_load() {
    if !runtime_kit_available() {
        return;
    }
    let project = Project::new("required", REQUIRED_FIXTURE, true);
    let output = project.beskid(&["test", "--target", "Tests"]);
    let log = text(&output);
    assert!(!output.status.success(), "a required missing library must fail:\n{log}");
    assert!(log.contains("extern resolve: dlopen(libbeskid-not-installed.so.1)"), "unexpected load failure:\n{log}");

    let (output, _) = project.build("required_app");
    assert!(!output.status.success(), "a required missing library must not link:\n{}", text(&output));
}

#[test]
fn non_boolean_optional_argument_is_rejected() {
    let project = Project::new(
        "bad_optional",
        "[Extern(Abi:\"C\", Library:\"libc.so.6\", Optional:\"yes\")]\npub contract Native {\n    i64 labs(i64 \
         value);\n}\n\npub i64 Main() {\n    return Native.labs(1);\n}\n",
        false,
    );
    let output = project.beskid(&["analyze"]);
    let log = text(&output);
    assert!(!output.status.success(), "a non-boolean Optional must not analyze:\n{log}");
    assert!(
        log.contains("invalid optional extern contract") && log.contains("Optional takes a boolean literal"),
        "missing T0905:\n{log}"
    );
}

#[test]
fn availability_query_must_be_bool_without_parameters() {
    let project = Project::new(
        "bad_available",
        "[Extern(Abi:\"C\", Library:\"libc.so.6\", Optional:true)]\npub contract Native {\n    i64 \
         Available(i64 value);\n    i64 labs(i64 value);\n}\n\npub i64 Main() {\n    return Native.labs(1);\n}\n",
        false,
    );
    let output = project.beskid(&["analyze"]);
    let log = text(&output);
    assert!(!output.status.success(), "a malformed Available must not analyze:\n{log}");
    assert!(
        log.contains("invalid optional extern contract") && log.contains("bool Available();"),
        "missing T0905:\n{log}"
    );
}
