use super::*;

#[test]
fn executable_build_rejects_empty_entrypoint() {
    let artifact = lower_sample_artifact();
    let dir = temp_case_dir("entrypoint_required_exe");
    let output = dir.join("sample");

    let err = build(AotBuildRequest::with_defaults(artifact, BuildOutputKind::Exe, output, "   "))
        .expect_err("blank entrypoint for executable should fail");

    assert!(matches!(err, AotError::InvalidRequest { .. }));
    assert!(err.to_string().contains("entrypoint must not be empty"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn object_only_build_allows_empty_entrypoint() {
    let artifact = lower_sample_artifact();
    let dir = temp_case_dir("entrypoint_not_required_object");
    let output = dir.join("sample.o");

    let result = build(AotBuildRequest::with_defaults(artifact, BuildOutputKind::ObjectOnly, output, "   "))
        .expect("object-only build should not require entrypoint");

    assert!(result.object_path.exists(), "expected object file to exist");
    assert!(result.final_path.is_none(), "object-only should not produce final output");
    let _ = std::fs::remove_dir_all(dir);
}
