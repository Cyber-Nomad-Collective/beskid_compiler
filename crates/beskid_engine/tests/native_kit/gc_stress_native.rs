#![cfg(unix)]

//! GC stress mode, heap verification, debug-kit poisoning and quarantine
//! (`v061-gc-stress-and-poison`, tasks 2.1-2.6).
//!
//! `tests/fixtures/gc_stress.c` is linked against the static and the shared library of the shared
//! debug kit. The runtime reads `BESKID_GC_STRESS` and `BESKID_GC_QUARANTINE` in
//! `beskid_rt_v5_process_init`, so every case sets them in the child's environment.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use beskid_abi::abi_v5::{TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS};

use crate::shared_kit;

struct Fixture {
    _scratch: tempfile::TempDir,
    executables: Vec<(&'static str, PathBuf)>,
    library_directory: PathBuf,
}

impl Fixture {
    fn build(kit: &beskid_abi::runtime_kit::ResolvedRuntimeKit) -> Self {
        let scratch = tempfile::tempdir().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let source = root.join("tests/fixtures/gc_stress.c");
        let include = root.join("../beskid_abi/include");
        let mut executables = Vec::new();
        for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
            let executable = scratch.path().join(mode);
            let output = Command::new("cc")
                .arg("-std=c11")
                .arg("-I")
                .arg(&include)
                .arg(&source)
                .arg(library)
                .arg("-lpthread")
                .arg("-lm")
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            executables.push((mode, executable));
        }
        let library_directory = kit.shared_library.parent().unwrap().to_path_buf();
        Self { _scratch: scratch, executables, library_directory }
    }

    fn run(&self, executable: &Path, mode: &str, environment: &[(&str, &str)]) -> Output {
        let library_path_variable = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
        let mut command = Command::new(executable);
        command.arg(mode).env(library_path_variable, &self.library_directory);
        command.env_remove("BESKID_GC_STRESS").env_remove("BESKID_GC_QUARANTINE");
        for (key, value) in environment {
            command.env(key, value);
        }
        command.output().unwrap()
    }
}

fn assert_trap(label: &str, output: &Output, expected: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.code() == Some(i32::from(TRAP_EXIT_STATUS)) && stderr.contains(expected),
        "{label}: expected exit {TRAP_EXIT_STATUS} and `{expected}`; got {:?}, stdout `{}`, stderr `{}`",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).trim(),
        stderr.trim()
    );
}

fn assert_success(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label}: expected success; got {:?}, stdout `{}`, stderr `{}`",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

/// (a) A missing root is found by `alloc` stress and not without it; `alloc:1` is the same mode.
#[test]
fn alloc_stress_exposes_a_missing_root_that_ordinary_pacing_hides() {
    let shared = shared_kit::debug();
    let fixture = Fixture::build(shared.kit());
    let expected = format!(
        "{TRAP_DIAGNOSTIC_PREFIX}: invalid_or_stale_handle (6): str_concat: managed string operand is not live (use after collection)"
    );
    for (mode, executable) in &fixture.executables {
        assert_success(&format!("{mode}/no stress"), &fixture.run(executable, "missing-root", &[]));
        for stress in ["alloc", "alloc:1", "verify,alloc"] {
            let output = fixture.run(executable, "missing-root", &[("BESKID_GC_STRESS", stress)]);
            assert_trap(&format!("{mode}/{stress}"), &output, &expected);
        }
    }
}

/// (b) `verify` runs the verifier before a collection and traps on a corrupted heap.
#[test]
fn verify_stress_traps_on_a_corrupted_heap_before_the_collection() {
    let shared = shared_kit::debug();
    let fixture = Fixture::build(shared.kit());
    let expected = format!(
        "{TRAP_DIAGNOSTIC_PREFIX}: runtime_internal_corruption (10): gc_heap_verify failed before a collection (BESKID_GC_STRESS verify)"
    );
    for (mode, executable) in &fixture.executables {
        assert_success(&format!("{mode}/no verify"), &fixture.run(executable, "verify-corrupt", &[]));
        let output = fixture.run(executable, "verify-corrupt", &[("BESKID_GC_STRESS", "verify")]);
        assert_trap(&format!("{mode}/verify"), &output, &expected);
    }
}

/// (c) The debug kit quarantines the freed slot, so a dangling reference traps through the
/// fail-fast liveness check instead of reading the object that reused the slot.
#[test]
fn debug_kit_quarantine_keeps_a_dangling_reference_detectable() {
    let shared = shared_kit::debug();
    let fixture = Fixture::build(shared.kit());
    let expected = format!(
        "{TRAP_DIAGNOSTIC_PREFIX}: invalid_or_stale_handle (6): str_len: managed string operand is not live (use after collection)"
    );
    for (mode, executable) in &fixture.executables {
        let profile = fixture.run(executable, "profile", &[]);
        assert_success(&format!("{mode}/profile"), &profile);
        assert!(
            String::from_utf8_lossy(&profile.stdout).contains("kit-profile=1"),
            "{mode}: the debug kit must report the debug build profile (platform object compiled with \
             BESKID_RUNTIME_KIT_DEBUG=1); got `{}`",
            String::from_utf8_lossy(&profile.stdout).trim()
        );
        let output = fixture.run(executable, "quarantine", &[]);
        assert_trap(&format!("{mode}/quarantine"), &output, &expected);
        // A budget below one slot holds nothing back: poisoning alone stays on and the run still
        // verifies clean, whether or not the freed slot was reused.
        let unbudgeted = fixture.run(executable, "init", &[("BESKID_GC_QUARANTINE", "0")]);
        assert_success(&format!("{mode}/quarantine budget 0"), &unbudgeted);
    }
}

/// (d) Malformed settings terminate initialization with the variable name.
#[test]
fn malformed_gc_stress_settings_are_rejected_at_process_init() {
    let shared = shared_kit::debug();
    let fixture = Fixture::build(shared.kit());
    for (mode, executable) in &fixture.executables {
        for value in ["often", "alloc:0", "alloc:", "alloc,alloc", "alloc,", ",verify", "verify,verify", "alloc:1x"] {
            let output = fixture.run(executable, "init", &[("BESKID_GC_STRESS", value)]);
            assert_trap(&format!("{mode}/BESKID_GC_STRESS={value}"), &output, "BESKID_GC_STRESS");
        }
        let output = fixture.run(executable, "init", &[("BESKID_GC_QUARANTINE", "lots")]);
        assert_trap(&format!("{mode}/BESKID_GC_QUARANTINE=lots"), &output, "BESKID_GC_QUARANTINE");
        for value in ["", "alloc", "alloc:7", "verify", "alloc:3,verify"] {
            assert_success(
                &format!("{mode}/BESKID_GC_STRESS={value}"),
                &fixture.run(executable, "init", &[("BESKID_GC_STRESS", value)]),
            );
        }
    }
}

/// (e) Without the variables the stress machinery stays off: the unrooted string survives until
/// an ordinary collection, exactly as before the change.
#[test]
fn without_the_variables_pacing_is_unchanged() {
    let shared = shared_kit::debug();
    let fixture = Fixture::build(shared.kit());
    for (mode, executable) in &fixture.executables {
        let output = fixture.run(executable, "missing-root", &[]);
        assert_success(&format!("{mode}/unset"), &output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("joined=14"), "{mode}: {output:?}");
    }
}
