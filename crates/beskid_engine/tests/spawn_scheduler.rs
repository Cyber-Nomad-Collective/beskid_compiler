use std::path::Path;

use beskid_engine::services::run_entrypoint;

#[test]
fn jit_runs_spawn_under_fiber_scheduler() {
    let source = "i64 child_value() { return 42; } unit Main() { spawn child_value; }";
    run_entrypoint(Path::new("spawn_scheduler.bd"), source, "Main")
        .expect("spawn should execute under the fiber scheduler without crashing");
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
