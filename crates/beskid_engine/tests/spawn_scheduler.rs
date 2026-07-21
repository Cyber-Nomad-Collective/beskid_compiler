use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::{prepare_jit_entrypoint, run_entrypoint};
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

#[test]
#[ignore = "blocked by CYB-129: macOS arm64 JIT-to-canonical-runtime dylib call raises SIGILL"]
fn jit_runs_zero_capture_lambda_spawn_under_fiber_scheduler() {
    let prefix = tempfile::tempdir().expect("exact runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish exact native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug)
        .expect("load exact native runtime kit");

    let source = "i64 Main() { spawn (() => 42_i64); return 5; }";
    let prepared = prepare_jit_entrypoint(Path::new("spawn_lambda.bd"), source, "Main")
        .expect("syntax-owned lambda spawn must prepare for JIT");
    engine
        .compile_artifact(&prepared.artifact)
        .expect("syntax-owned lambda spawn must compile in the JIT");
    let pointer = unsafe { engine.entrypoint_ptr(&prepared.symbol) }.expect("Main pointer");
    let main: extern "C" fn() -> i64 = unsafe { std::mem::transmute(pointer) };
    assert_eq!(
        main(),
        5,
        "spawned lambda must not corrupt the caller result"
    );
}

#[test]
fn jit_child_value_returns_42() {
    let source = "i64 child_value() { return 42; } i64 Main() { return child_value(); }";
    let output =
        run_entrypoint(Path::new("child_value.bd"), source, "Main").expect("main should run");
    assert_eq!(
        output, "42",
        "expected child_value return to round-trip through JIT"
    );
}
