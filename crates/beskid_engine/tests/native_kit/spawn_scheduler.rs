use crate::shared_kit;

use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::{prepare_jit_entrypoint, run_entrypoint};
use beskid_engine::{Engine, host_runtime_target};
use crate::runtime_prefix::RuntimePrefixContext;

#[test]
fn jit_runs_zero_capture_lambda_spawn_under_fiber_scheduler() {
    let kit_lease = shared_kit::debug();
    let prefix = kit_lease.prefix();
    let target = host_runtime_target().expect("supported native host target");
    let mut engine =
        Engine::with_runtime_kit(prefix, target, BuildProfile::Debug).expect("load exact native runtime kit");

    let source = "pub type Fiber<T> { i64 handle, } i64 Main() { let child = spawn (() => 42_i64); return 5; }";
    let prepared = prepare_jit_entrypoint(Path::new("spawn_lambda.bd"), source, "Main")
        .expect("syntax-owned lambda spawn must prepare for JIT");
    engine.compile_artifact(&prepared.artifact).expect("syntax-owned lambda spawn must compile in the JIT");
    let pointer = unsafe { engine.entrypoint_ptr(&prepared.symbol) }.expect("Main pointer");
    let main: extern "C" fn() -> i64 = unsafe { std::mem::transmute(pointer) };
    assert_eq!(main(), 5, "spawned lambda must not corrupt the caller result");
}

#[test]
fn jit_child_value_returns_42() {
    let kit_lease = shared_kit::debug();
    let prefix = kit_lease.prefix();
    let _runtime_prefix = RuntimePrefixContext::install(prefix);

    let source = "i64 child_value() { return 42; } i64 Main() { return child_value(); }";
    let output = run_entrypoint(Path::new("child_value.bd"), source, "Main").expect("main should run");
    assert_eq!(output, "42", "expected child_value return to round-trip through JIT");
}
