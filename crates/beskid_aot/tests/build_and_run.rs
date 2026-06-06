use std::fs;

use beskid_aot::{
    AotRunRequest, BuildProfile, RuntimeStrategy, build_and_run, default_runtime_strategy,
};
use beskid_codegen::lower_source;

#[test]
fn build_and_run_executes_linked_executable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = "i32 main() { return 7; }";
    fs::write(&source_path, source).expect("write source");

    let lowered = lower_source(&source_path, source, false).expect("lower fixture");
    beskid_codegen::validate_artifact(&lowered.artifact).expect("validate link plan");

    let runtime = default_runtime_strategy(BuildProfile::Debug, None)
        .unwrap_or(RuntimeStrategy::Standalone);

    let output_dir = temp.path().join("out");
    let result = build_and_run(AotRunRequest {
        artifact: lowered.artifact,
        entrypoint: "main".to_owned(),
        output_dir: output_dir.clone(),
        runtime,
    })
    .expect("build and run executable");

    assert!(
        result.exe_path.exists(),
        "expected linked executable at {}",
        result.exe_path.display()
    );
    assert_eq!(result.exit_code, 7, "expected main return value as exit code");
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[test]
fn build_and_run_executes_str_len() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = "i64 main() { return __str_len(\"hello\"); }";
    std::fs::write(&source_path, source).expect("write source");

    let lowered = lower_source(&source_path, source, false).expect("lower fixture");
    let runtime = default_runtime_strategy(BuildProfile::Debug, None)
        .unwrap_or(RuntimeStrategy::Standalone);

    let output_dir = temp.path().join("out");
    let result = build_and_run(AotRunRequest {
        artifact: lowered.artifact,
        entrypoint: "main".to_owned(),
        output_dir: output_dir.clone(),
        runtime,
    })
    .expect("build and run");

    assert_eq!(result.exit_code, 5, "expected str_len builtin result as exit code");
}
