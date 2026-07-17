//! AOT compile and execute helpers shared by runtime integration tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use abfall::Heap;
use beskid_aot::{AotRunRequest, BuildProfile, build_and_run, default_runtime_strategy};
use beskid_codegen::lower_source;
use beskid_runtime::{
    RuntimeRoot, clear_current_heap, clear_current_root, enter_runtime_scope, leave_runtime_scope,
    scheduler_init, set_current_heap, set_current_root,
};

use crate::test_harness::temp_case_dir;

const TEST_SOURCE_PATH: &str = "<beskid_tests>";

pub fn compile_artifact(source: &str) -> beskid_codegen::CodegenArtifact {
    let lowered = lower_source(Path::new(TEST_SOURCE_PATH), source, false)
        .expect("expected codegen lowering to succeed");
    lowered.artifact
}

pub fn validate_lowered(source: &str) {
    let artifact = compile_artifact(source);
    beskid_codegen::validate_artifact(&artifact).expect("expected artifact validation to succeed");
}

pub fn build_aot_exe(source: &str, case_name: &str) -> (PathBuf, beskid_aot::AotRunResult) {
    let artifact = compile_artifact(source);
    let output_dir = temp_case_dir(case_name);
    let runtime = default_runtime_strategy(BuildProfile::Debug, None)
        .expect("tests that link executables require an installed ABI-v5 runtime kit");
    let result = build_and_run(AotRunRequest {
        artifact,
        entrypoint: "Main".to_owned(),
        output_dir: output_dir.clone(),
        runtime,
    })
    .expect("expected AOT build and run to succeed");
    (output_dir, result)
}

pub fn aot_run_main_i64(source: &str) -> i64 {
    let (dir, result) = build_aot_exe(source, "aot_run_main_i64");
    let value = i64::from(result.exit_code);
    let _ = std::fs::remove_dir_all(dir);
    value
}

pub fn aot_run_main_i32(source: &str) -> i32 {
    let (dir, result) = build_aot_exe(source, "aot_run_main_i32");
    let value = result.exit_code;
    let _ = std::fs::remove_dir_all(dir);
    value
}

pub fn aot_compile_only(source: &str) {
    validate_lowered(source);
}

/// Run `f` with TLS pointing at a fresh heap session and runtime root (no JIT).
pub fn with_runtime_scope<R>(f: impl FnOnce(&Arc<Heap>, &mut RuntimeRoot) -> R) -> R {
    scheduler_init();
    let heap = Arc::new(Heap::off());
    let mut root = RuntimeRoot::new(Arc::clone(&heap));

    enter_runtime_scope();
    set_current_heap(&heap);
    set_current_root(&mut root as *mut _);
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            clear_current_heap();
            clear_current_root();
            leave_runtime_scope();
        }
    }
    let _guard = Guard;
    f(&heap, &mut root)
}
