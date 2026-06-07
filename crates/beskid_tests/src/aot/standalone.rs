use super::*;

#[test]
fn executable_build_succeeds_in_standalone_mode() {
    let artifact = lower_sample_artifact();
    let dir = temp_case_dir("standalone_executable");
    let output = dir.join("sample_standalone");

    let result = build(AotBuildRequest {
        runtime: RuntimeStrategy::Standalone,
        ..AotBuildRequest::with_defaults(artifact, BuildOutputKind::Exe, output, "Main")
    })
    .expect("standalone executable build");

    let final_path = result
        .final_path
        .expect("executable build should emit final output");
    assert!(
        final_path.exists(),
        "expected standalone executable to exist"
    );

    let _ = std::fs::remove_dir_all(dir);
}
