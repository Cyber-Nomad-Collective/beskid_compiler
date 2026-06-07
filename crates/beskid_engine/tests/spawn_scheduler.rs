use std::path::Path;

use beskid_engine::services::run_entrypoint;

#[test]
fn jit_runs_spawn_under_fiber_scheduler() {
    let source = "i64 child_value() { return 42; } unit main() { spawn child_value; }";
    run_entrypoint(Path::new("spawn_scheduler.bd"), source, "main")
        .expect("spawn should execute under the fiber scheduler without crashing");
}

#[test]
fn jit_child_value_returns_42() {
    let source = "i64 child_value() { return 42; } i64 main() { return child_value(); }";
    let output = run_entrypoint(Path::new("child_value.bd"), source, "main").expect("main should run");
    assert_eq!(output, "42", "expected child_value return to round-trip through JIT");
}
