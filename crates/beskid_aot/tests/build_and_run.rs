use std::fs;

use beskid_aot::{
    AotRunRequest, RuntimeStrategy, build_and_run,
};
use beskid_codegen::{lower_source_for_entrypoint, validate_artifact};

#[test]
fn build_and_run_executes_linked_executable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = "i32 Main() { return 7; }";
    fs::write(&source_path, source).expect("write source");

    let lowered =
        lower_source_for_entrypoint(&source_path, source, "Main", false, None).expect("lower fixture");
    validate_artifact(&lowered.artifact).expect("validate link plan");

    let output_dir = temp.path().join("out");
    let result = build_and_run(AotRunRequest {
        artifact: lowered.artifact,
        entrypoint: "Main".to_owned(),
        output_dir: output_dir.clone(),
        runtime: RuntimeStrategy::Standalone,
    })
    .expect("build and run executable");

    assert!(
        result.exe_path.exists(),
        "expected linked executable at {}",
        result.exe_path.display()
    );
    assert_eq!(
        result.exit_code, 7,
        "expected main return value as exit code"
    );
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}
