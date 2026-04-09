use std::time::{Duration, Instant};

use crate::harness::assertions::{
    assert_exit_code, assert_file_exists, assert_output_contains, assert_success,
};
use crate::harness::cli::BeskidCliInvoker;
use crate::harness::process::run_binary;
use crate::harness::workspace::E2eWorkspace;

#[test]
fn perf_smoke_aot_build_and_run_stay_within_time_budget() {
    let workspace = E2eWorkspace::from_fixture("perf_loop");
    let manifest = workspace.join("Project.proj");
    let output_binary = workspace.join("out/perf_loop");
    let cli = BeskidCliInvoker::new();

    let build_max_ms = read_budget_ms("BESKID_E2E_BUILD_MAX_MS", 180_000);
    let run_max_ms = read_budget_ms("BESKID_E2E_RUN_MAX_MS", 30_000);

    let build_start = Instant::now();
    let build = cli.run([
        "build",
        "--project",
        manifest.to_str().expect("manifest path str"),
        "--target",
        "App",
        "--output",
        output_binary.to_str().expect("output path str"),
        "--release",
    ]);
    let build_elapsed = build_start.elapsed();
    assert_success(&build, "build perf-loop fixture");
    assert_output_contains(&build, "output:", "build perf-loop fixture");
    assert_file_exists(&output_binary, "perf-loop output binary");
    assert!(
        build_elapsed <= Duration::from_millis(build_max_ms),
        "build perf budget exceeded: {:?} > {}ms",
        build_elapsed,
        build_max_ms
    );

    let run_start = Instant::now();
    let run = run_binary(
        &output_binary,
        Duration::from_millis(run_max_ms.saturating_mul(2)),
    );
    let run_elapsed = run_start.elapsed();
    assert_success(&run, "execute perf-loop binary");
    assert_exit_code(&run, 0, "execute perf-loop binary");
    assert!(
        run_elapsed <= Duration::from_millis(run_max_ms),
        "runtime perf budget exceeded: {:?} > {}ms",
        run_elapsed,
        run_max_ms
    );
}

#[test]
fn perf_batch_release_builds_stay_within_total_budget() {
    let cli = BeskidCliInvoker::new();
    let fixtures = [
        "smoke_project",
        "contracts_dispatch",
        "method_dispatch",
        "closure_capture",
    ];
    let total_build_budget_ms = read_budget_ms("BESKID_E2E_BATCH_BUILD_MAX_MS", 240_000);

    let batch_start = Instant::now();
    for fixture in fixtures {
        let workspace = E2eWorkspace::from_fixture(fixture);
        let manifest = workspace.join("Project.proj");
        let output_binary = workspace.join(format!("out/{fixture}_release"));

        let build = cli.run([
            "build",
            "--project",
            manifest.to_str().expect("manifest path str"),
            "--target",
            "App",
            "--output",
            output_binary.to_str().expect("output path str"),
            "--release",
        ]);
        assert_success(&build, "batch release build fixture");
        assert_file_exists(&output_binary, "batch release build output");
    }
    let elapsed = batch_start.elapsed();
    assert!(
        elapsed <= Duration::from_millis(total_build_budget_ms),
        "batch build budget exceeded: {:?} > {}ms",
        elapsed,
        total_build_budget_ms
    );
}

fn read_budget_ms(env_name: &str, fallback: u64) -> u64 {
    std::env::var(env_name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(fallback)
}
