//! Execution proof for a runtime kit published by the production native-host builder.

use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source};
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

#[test]
fn fresh_native_runtime_kit_executes_a_canonical_entrypoint() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine =
        Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).expect("load the exact fresh runtime kit");

    let source = "i64 Main() { return 41 + 1; }";
    let source_path =
        beskid_codegen::materialize_source_path_for_lowering(Path::new("native-runtime-kit-smoke.bd"), source)
            .expect("materialize canonical source");
    let resolved = resolved_input_from_plan(
        source_path,
        source.to_owned(),
        synthetic_compile_plan_for_source(Path::new("native-runtime-kit-smoke.bd")),
        None,
        None,
    );
    let front = beskid_queries::compile_mod_free_front_end(&resolved, FrontEndOptions::default(), None)
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

#[test]
fn fresh_native_runtime_kit_accepts_one_explicit_argument_handoff() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine =
        Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).expect("load the exact fresh runtime kit");

    engine
        .initialize_arguments(&["beskid-test".to_owned(), "--explicit".to_owned()])
        .expect("handoff explicit host-owned arguments");
    let error = engine
        .initialize_arguments(&["replacement".to_owned()])
        .expect_err("argument authority is one-shot per engine");
    assert!(error.to_string().contains("already initialized"));
}

#[test]
fn public_run_entrypoint_uses_the_syntax_isle_path() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine =
        Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).expect("load the exact fresh runtime kit");

    let output = beskid_engine::services::run_entrypoint_with_engine(
        &mut engine,
        Path::new("public-syntax-entrypoint.bd"),
        "i64 Main() { return 41 + 1; }",
        "Main",
        &beskid_engine::ModHost::NoMods,
    )
    .expect("public entrypoint executes through syntax ISLE");
    assert_eq!(output, "42");
}

#[test]
#[ignore = "staged evidence: needs BESKID_RUNTIME_PREFIX and BESKID_RUNTIME_KIT_PROFILE"]
fn staged_runtime_kit_executes_a_canonical_entrypoint() {
    let staged = beskid_tests_support::evidence::staged_kit();
    let prefix = staged.prefix;
    let profile = match staged.profile {
        beskid_tests_support::evidence::StagedProfile::Debug => BuildProfile::Debug,
        beskid_tests_support::evidence::StagedProfile::Release => BuildProfile::Release,
    };
    let target = host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(&prefix, target, profile).expect("load the staged Linux runtime kit");
    let source = "i64 Main() { return 41 + 1; }";
    let source_path =
        beskid_codegen::materialize_source_path_for_lowering(Path::new("staged-runtime-kit-smoke.bd"), source)
            .expect("materialize canonical source");
    let resolved = resolved_input_from_plan(
        source_path,
        source.to_owned(),
        synthetic_compile_plan_for_source(Path::new("staged-runtime-kit-smoke.bd")),
        None,
        None,
    );
    let front = beskid_queries::compile_mod_free_front_end(&resolved, FrontEndOptions::default(), None)
        .expect("prepare canonical entrypoint");
    let output = run_entrypoint_from_front_end_with_engine(
        &mut engine,
        &front,
        "staged-runtime-kit-smoke.bd",
        source,
        "Main",
        None,
    )
    .expect("execute against the staged Linux runtime kit");
    assert_eq!(output, "42");
}
