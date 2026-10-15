#![cfg(unix)]

//! Fail-fast liveness (owner decision 2026-10-15): a runtime service that receives a managed
//! operand which a collection already freed traps `invalid_or_stale_handle` with a diagnostic
//! naming the service. It never returns a substitute value; before this rule `str_concat` and
//! `str_slice` returned null, `str_len` and the comparisons read freed memory, and the root-handle
//! services reported an ordinary failed root.
//!
//! `tests/fixtures/use_after_collection.c` is linked against the static and the shared runtime
//! library. Each child process frees one operand with `gc_collect` and passes it to one service.

use std::{path::Path, process::Command};

use beskid_abi::abi_v5::{TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS};

use crate::shared_kit;

/// Service named in the diagnostic, and whether it takes a string operand.
const SERVICES: &[(&str, bool)] = &[
    ("str_len", true),
    ("str_eq", true),
    ("str_cmp", true),
    ("str_concat", true),
    ("str_slice", true),
    ("utf8_record_adopt", true),
    ("gc_root_handle", false),
    ("gc_try_root_handle", false),
    ("abi_value_initialize", false),
];

#[test]
fn dead_managed_operands_trap_with_a_diagnostic_in_static_and_shared_kits() {
    let scratch = tempfile::tempdir().unwrap();
    let shared = shared_kit::debug();
    let kit = shared.kit();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = root.join("tests/fixtures/use_after_collection.c");
    let include = root.join("../beskid_abi/include");
    let library_path_variable = if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
    let library_directory = kit.shared_library.parent().unwrap();
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

        let live = Command::new(&executable).arg("live").env(library_path_variable, library_directory).output().unwrap();
        assert!(
            live.status.success(),
            "{mode}: rooted operands must survive the collection and the services must succeed: {live:?}"
        );

        let mut violations = Vec::new();
        for (service, string) in SERVICES {
            let run =
                Command::new(&executable).arg(service).env(library_path_variable, library_directory).output().unwrap();
            let stderr = String::from_utf8_lossy(&run.stderr);
            let subject = if *string { "managed string operand" } else { "managed operand" };
            let expected = format!(
                "{TRAP_DIAGNOSTIC_PREFIX}: invalid_or_stale_handle (6): {service}: {subject} is not live (use after collection)"
            );
            if run.status.code() != Some(i32::from(TRAP_EXIT_STATUS)) || !stderr.contains(&expected) {
                violations.push(format!(
                    "{mode}/{service}: expected exit {TRAP_EXIT_STATUS} and `{expected}`; got {:?}, stdout `{}`, stderr `{}`",
                    run.status.code(),
                    String::from_utf8_lossy(&run.stdout).trim(),
                    stderr.trim()
                ));
            }
        }
        assert!(violations.is_empty(), "a dead operand must trap, not return a substitute:\n{}", violations.join("\n"));
    }
}
