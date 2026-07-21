use super::*;

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn object_only_build_emits_object_file() {
    let artifact = lower_sample_artifact();
    let dir = temp_case_dir("object_only");
    let output = dir.join("sample.o");

    let result = build(AotBuildRequest::with_defaults(
        artifact,
        BuildOutputKind::ObjectOnly,
        output.clone(),
        "main",
    ))
    .expect("aot object build");

    assert!(result.object_path.exists(), "expected object file to exist");
    assert!(
        result.final_path.is_none(),
        "object-only build must not link"
    );

    let _ = std::fs::remove_dir_all(dir);
}
