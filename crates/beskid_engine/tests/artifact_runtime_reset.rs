//! Artifact replacement must not leave runtime heap objects pointing into retired JIT data.

use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source};
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

#[test]
fn replacing_artifacts_retains_descriptor_storage_owned_by_the_persistent_runtime() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine =
        Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).expect("load exact runtime kit");

    let source = r#"
i64 AllocateArray() {
    i64[] values = [41_i64];
    return values[0];
}
i64 RetireDescriptorOwner() { return 1_i64; }
i64 AllocateAfterReplacement() {
    string[] values = ["live"];
    return 42_i64;
}
"#;
    let source_path =
        beskid_codegen::materialize_source_path_for_lowering(Path::new("artifact-runtime-reset.bd"), source)
            .expect("materialize canonical source");
    let resolved = resolved_input_from_plan(
        source_path,
        source.to_owned(),
        synthetic_compile_plan_for_source(Path::new("artifact-runtime-reset.bd")),
        None,
        None,
    );
    let front = beskid_queries::compile_front_end_from_resolved_input(&resolved, FrontEndOptions::default(), None)
        .expect("prepare canonical entrypoints");

    for (entrypoint, expected) in
        [("AllocateArray", "41"), ("RetireDescriptorOwner", "1"), ("AllocateAfterReplacement", "42")]
    {
        let output = run_entrypoint_from_front_end_with_engine(
            &mut engine,
            &front,
            "artifact-runtime-reset.bd",
            source,
            entrypoint,
            None,
        )
        .unwrap_or_else(|error| panic!("{entrypoint} executes after artifact replacement: {error}"));
        assert_eq!(output, expected);
    }
}
