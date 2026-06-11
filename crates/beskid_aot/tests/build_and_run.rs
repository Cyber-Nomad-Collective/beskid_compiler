use std::fs;

use beskid_aot::{
    AotRunRequest, BuildProfile, RuntimeLinkProfile, RuntimeStrategy, build_and_run,
    default_runtime_strategy,
};
use beskid_codegen::{lower_source_for_entrypoint, validate_artifact};

#[test]
fn build_and_run_executes_linked_executable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = "i32 Main() { return 7; }";
    fs::write(&source_path, source).expect("write source");

    let lowered =
        lower_source_for_entrypoint(&source_path, source, "Main", false).expect("lower fixture");
    validate_artifact(&lowered.artifact).expect("validate link plan");

    let runtime = default_runtime_strategy(BuildProfile::Debug, None, RuntimeLinkProfile::Std)
        .unwrap_or(RuntimeStrategy::Standalone);

    let output_dir = temp.path().join("out");
    let result = build_and_run(AotRunRequest {
        artifact: lowered.artifact,
        entrypoint: "Main".to_owned(),
        output_dir: output_dir.clone(),
        runtime,
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

#[test]
fn build_and_run_executes_str_len() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = "i64 Main() { return __str_len(\"hello\"); }";
    std::fs::write(&source_path, source).expect("write source");

    let lowered =
        lower_source_for_entrypoint(&source_path, source, "Main", false).expect("lower fixture");
    let runtime = default_runtime_strategy(BuildProfile::Debug, None, RuntimeLinkProfile::Std)
        .unwrap_or(RuntimeStrategy::Standalone);

    let output_dir = temp.path().join("out");
    let result = build_and_run(AotRunRequest {
        artifact: lowered.artifact,
        entrypoint: "Main".to_owned(),
        output_dir: output_dir.clone(),
        runtime,
    })
    .expect("build and run");

    assert_eq!(
        result.exit_code, 5,
        "expected str_len builtin result as exit code"
    );
}

#[test]
fn host_archive_resolves_in_dev() {
    use beskid_aot::{resolve_bundled_host_archive, BuildProfile};
    let path = resolve_bundled_host_archive(BuildProfile::Debug, None)
        .expect("host archive should resolve");
    assert!(path.is_file(), "missing {}", path.display());
}

#[test]
fn std_build_links_host_archive() {
    use beskid_aot::{
        AotBuildRequest, BuildOutputKind, BuildProfile, RuntimeLinkProfile, build,
        default_runtime_strategy,
    };

    let temp = tempfile::tempdir().expect("tempdir");
    let source_path = temp.path().join("main.bd");
    let source = "i32 Main() { return 7; }";
    fs::write(&source_path, source).expect("write source");

    let lowered =
        lower_source_for_entrypoint(&source_path, source, "Main", false).expect("lower fixture");
    let runtime = default_runtime_strategy(BuildProfile::Debug, None, RuntimeLinkProfile::Std)
        .expect("runtime");
    let exe_path = temp.path().join("beskid_run");

    let result = build(AotBuildRequest {
        runtime,
        runtime_link_profile: RuntimeLinkProfile::Std,
        verbose_link: true,
        ..AotBuildRequest::with_defaults(
            lowered.artifact,
            BuildOutputKind::Exe,
            exe_path.clone(),
            "Main",
        )
    })
    .expect("build should succeed");

    let invocation = result
        .linker_invocation
        .expect("linker invocation should be recorded");
    assert!(
        invocation.contains("libbeskid_runtime_bridge"),
        "expected runtime bridge in link line, got: {invocation}"
    );
    assert!(exe_path.is_file(), "expected executable at {}", exe_path.display());
}
