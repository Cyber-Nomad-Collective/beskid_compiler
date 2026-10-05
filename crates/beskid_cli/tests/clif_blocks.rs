//! End-to-end `clif { ... }` acceptance through the CLI for an ordinary (non-Corelib) project.
//!
//! The fixture uses CLIF blocks for `bxor`, 64x64->high `umulhi`, `rotl` on `u32`, a `u32[]`
//! payload load/xor/store, an `i32x4` SIMD lane sum, `length`, a typed `let` context, and a
//! `call @labs` authorized only by a declaration-only C-ABI `[Extern]` contract.
//!
//! The JIT (`beskid test`) and AOT (`beskid build` + run) cases need an installed exact runtime
//! kit with debug and release profiles; they run when `BESKID_RUNTIME_PREFIX` names one and are
//! skipped otherwise. Build one with
//! `beskid runtime-kit build-native-host --prefix <prefix> --profile debug` (and `release`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const FIXTURE: &str = include_str!("fixtures/clif_blocks/Main.bd");

struct ClifProject {
    root: PathBuf,
}

impl ClifProject {
    fn new(label: &str, source: &str, with_corelib: bool) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid_clif_{label}_{}_{nonce}", std::process::id()));
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
            root.join("ClifE2E.bproj"),
            format!(
                "ClifE2E {{\n  name = \"ClifE2E\"\n  version = \"0.1.0\"\n  root = \"src\"\n}}\n\n{dependency}\ntarget \
                 \"ClifApp\" {{\n  kind = App\n  entry = \"Main.bd\"\n}}\n\ntarget \"ClifTests\" {{\n  kind = Lib\n  \
                 entry = \"Main.bd\"\n}}\n"
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
            // A per-project managed Corelib keeps parallel cases from provisioning one shared root.
            .env("BESKID_CORELIB_ROOT", self.root.join("installed-corelib"))
            .current_dir(&self.root)
            .output()
            .expect("run beskid")
    }
}

impl Drop for ClifProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn runtime_kit_available() -> bool {
    let available = std::env::var_os("BESKID_RUNTIME_PREFIX").is_some_and(|prefix| !prefix.is_empty());
    if !available {
        eprintln!("skipping: BESKID_RUNTIME_PREFIX does not name an installed exact runtime kit");
    }
    available
}

#[test]
fn clif_blocks_pass_under_jit_test() {
    if !runtime_kit_available() {
        return;
    }
    let project = ClifProject::new("jit", FIXTURE, true);
    let output = project.beskid(&["test", "--target", "ClifTests"]);
    let log = text(&output);
    assert!(output.status.success(), "beskid test failed:\n{log}");
    assert!(log.contains("Result: passed=7, failed=0"), "unexpected test summary:\n{log}");
}

#[test]
fn clif_blocks_run_after_aot_build() {
    if !runtime_kit_available() {
        return;
    }
    let project = ClifProject::new("aot", FIXTURE, true);
    let executable = project.root.join(format!("clif_app{}", std::env::consts::EXE_SUFFIX));
    let output = project.beskid(&["build", "--target", "ClifApp", "--output", executable.to_str().expect("UTF-8 path")]);
    assert!(output.status.success(), "beskid build failed:\n{}", text(&output));
    let run = Command::new(&executable).output().expect("run AOT executable");
    assert_eq!(run.status.code(), Some(0), "AOT clif checks failed (exit code names the failing check):\n{}", text(&run));
}

#[test]
fn clif_block_without_typed_context_fails_with_e1232() {
    let project = ClifProject::new(
        "untyped",
        "pub i64 Main() {\n    clif { return %0 };\n    return 0;\n}\n",
        false,
    );
    let executable = project.root.join("untyped");
    let output = project.beskid(&["build", "--target", "ClifApp", "--output", executable.to_str().expect("UTF-8 path")]);
    let log = text(&output);
    assert!(!output.status.success(), "an untyped clif block must not build:\n{log}");
    assert!(log.contains("invalid clif block") && log.contains("needs a typed context"), "missing E1232:\n{log}");
}
