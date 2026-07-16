//! Execution proof for a runtime kit published by the production native-host builder.

use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::services::{
    FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

#[test]
fn fresh_native_runtime_kit_executes_a_canonical_entrypoint() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug)
        .expect("load the exact fresh runtime kit");

    let source = "i64 Main() { return 41 + 1; }";
    let source_path = beskid_codegen::materialize_source_path_for_lowering(
        Path::new("native-runtime-kit-smoke.bd"),
        source,
    )
    .expect("materialize canonical source");
    let resolved = resolved_input_from_plan(
        source_path,
        source.to_owned(),
        synthetic_compile_plan_for_source(Path::new("native-runtime-kit-smoke.bd")),
        None,
        None,
    );
    let front = beskid_queries::compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions::default(),
        None,
    )
    .expect("prepare canonical entrypoint");

    let output = run_entrypoint_from_front_end_with_engine(
        &mut engine,
        &front,
        "native-runtime-kit-smoke.bd",
        source,
        "Main",
        None,
    )
    .expect("execute against the fresh runtime kit");
    assert_eq!(output, "42");
}
