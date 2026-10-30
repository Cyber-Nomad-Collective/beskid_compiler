//! End-to-end `clif { ... }` acceptance through the CLI for an ordinary (non-Corelib) project.
//!
//! The fixture uses CLIF blocks for `bxor`, 64x64->high `umulhi`, `rotl` on `u32`, a `u32[]`
//! payload load/xor/store, an `i32x4` SIMD lane sum, `length`, a typed `let` context, a
//! `call @labs` authorized only by a declaration-only C-ABI `[Extern]` contract, loop-body,
//! assignment, and call-argument contexts, an expression-statement block, and an emulated
//! carry-in add. The native fixture hands payload pointers to libc and OpenSSL (Linux).
//!
//! The native test-runner (`beskid test`) and AOT (`beskid build` + run) cases run on the debug
//! runtime kit that `beskid_tests_mod_host::shared_runtime_prefix` stages once per target
//! directory; the CLI child receives it as `BESKID_RUNTIME_PREFIX`. No case reads the kit from the
//! test process environment, and none is skipped.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURE: &str = include_str!("fixtures/clif_blocks/Main.bd");
const NATIVE_FIXTURE: &str = include_str!("fixtures/clif_blocks/Native.bd");

/// An isolated host project. The Corelib comes from the implicit Core Corelib closure that the
/// per-project managed root provides; an explicit `source = path` dependency on the checkout
/// Corelib would copy `corelib_compiler_sdk` as a path package, which cannot authorize the
/// Corelib's native Mod adapters.
struct ClifProject {
    directory: tempfile::TempDir,
}

impl ClifProject {
    fn new(source: &str) -> Self {
        let directory = tempfile::tempdir().expect("project directory");
        let root = directory.path();
        fs::create_dir_all(root.join("Src")).expect("project source directory");
        fs::write(
            root.join("ClifE2E.bproj"),
            "ClifE2E {\n  name = \"ClifE2E\"\n  version = \"0.1.0\"\n  root = \"Src\"\n}\n\ntarget \"ClifApp\" {\n  \
             kind = App\n  entry = \"Main.bd\"\n}\n\ntarget \"ClifTests\" {\n  kind = Lib\n  entry = \"Main.bd\"\n}\n",
        )
        .expect("project manifest");
        fs::write(root.join("Src/Main.bd"), source).expect("project source");
        Self { directory }
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn beskid(&self, arguments: &[&str]) -> Output {
        let root = self.root();
        Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
            .args(arguments)
            .arg("--plain")
            .arg("--project")
            .arg(root.join("ClifE2E.bproj"))
            .env_remove("RUST_LOG")
            .env("BESKID_HOME", root.join("toolchain-home"))
            .env("BESKID_CONFIG_DIR", root.join("config"))
            // A per-project managed Corelib keeps parallel cases from provisioning one shared root.
            .env("BESKID_CORELIB_ROOT", root.join("installed-corelib"))
            // `beskid build` and `beskid test` select the debug profile of this exact kit.
            .env("BESKID_RUNTIME_PREFIX", shared_runtime_prefix())
            .env("BESKID_RUNTIME_KIT_PROFILE", "debug")
            .env("OTEL_SDK_DISABLED", "true")
            .current_dir(root)
            .output()
            .expect("run beskid")
    }

    fn executable(&self, name: &str) -> PathBuf {
        self.root().join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// The debug runtime kit shared by every test of this target directory, staged on first use.
fn shared_runtime_prefix() -> &'static Path {
    static PREFIX: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    PREFIX.get_or_init(|| beskid_tests_mod_host::shared_runtime_prefix().expect("stage the shared debug runtime kit"))
}

#[test]
fn clif_blocks_pass_under_jit_test() {
    let project = ClifProject::new(FIXTURE);
    let output = project.beskid(&["test", "--target", "ClifTests"]);
    let log = text(&output);
    assert!(output.status.success(), "beskid test failed:\n{log}");
    assert!(log.contains("Result: passed=10, failed=0"), "unexpected test summary:\n{log}");
}

#[test]
fn clif_blocks_run_after_aot_build() {
    let project = ClifProject::new(FIXTURE);
    let executable = project.executable("clif_app");
    let output =
        project.beskid(&["build", "--target", "ClifApp", "--output", executable.to_str().expect("UTF-8 path")]);
    assert!(output.status.success(), "beskid build failed:\n{}", text(&output));
    let run = Command::new(&executable).output().expect("run AOT executable");
    assert_eq!(
        run.status.code(),
        Some(0),
        "AOT clif checks failed (exit code names the failing check):\n{}",
        text(&run)
    );
}

#[test]
fn clif_block_without_typed_context_fails_with_e1232() {
    let project = ClifProject::new(
        "pub i64 Main() {\n    let value = clif {\n        %k = iconst.i64 1\n        return %k\n    };\n    return 0;\n}\n",
    );
    let executable = project.executable("untyped");
    let output =
        project.beskid(&["build", "--target", "ClifApp", "--output", executable.to_str().expect("UTF-8 path")]);
    let log = text(&output);
    assert!(!output.status.success(), "an untyped clif block must not build:\n{log}");
    assert!(log.contains("invalid clif block") && log.contains("needs a typed context"), "missing E1232:\n{log}");
}

/// Payload pointers handed to libc `memcpy`/`memset` and OpenSSL `SHA256` through C-ABI
/// `[Extern]` declarations. Needs `libcrypto.so.3` (and its link-time `libcrypto.so`), so it
/// runs on Linux only.
#[cfg(target_os = "linux")]
#[test]
fn clif_payload_handoff_to_native_code_under_jit_and_aot() {
    let project = ClifProject::new(NATIVE_FIXTURE);
    let output = project.beskid(&["test", "--target", "ClifTests"]);
    let log = text(&output);
    assert!(output.status.success(), "beskid test failed:\n{log}");
    assert!(log.contains("Result: passed=3, failed=0"), "unexpected test summary:\n{log}");

    let executable = project.executable("clif_native");
    let output =
        project.beskid(&["build", "--target", "ClifApp", "--output", executable.to_str().expect("UTF-8 path")]);
    assert!(output.status.success(), "beskid build failed:\n{}", text(&output));
    let run = Command::new(&executable).output().expect("run AOT executable");
    assert_eq!(run.status.code(), Some(0), "AOT native handoff failed (exit code names the check):\n{}", text(&run));
}

#[test]
fn clif_payload_handoff_to_non_extern_symbol_is_rejected() {
    let project = ClifProject::new(
        "pub unit Fill(u8[] bytes) {\n    clif {\n        %p = payload %0\n        %n = length %0\n        call \
         @memset(%p, %n, %n)\n    };\n    return;\n}\n\npub i64 Main() {\n    mut u8[] bytes = [0_u8];\n    \
         Fill(bytes);\n    return 0;\n}\n",
    );
    let executable = project.executable("rejected");
    let output =
        project.beskid(&["build", "--target", "ClifApp", "--output", executable.to_str().expect("UTF-8 path")]);
    let log = text(&output);
    assert!(!output.status.success(), "a payload handoff to an undeclared symbol must not build:\n{log}");
    assert!(log.contains("C-ABI `[Extern]` contract"), "missing handoff diagnostic:\n{log}");
}
