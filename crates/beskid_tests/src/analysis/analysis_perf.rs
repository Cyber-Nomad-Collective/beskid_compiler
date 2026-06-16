//! Analysis phase timing smoke (run with `cargo test -p beskid_tests analysis_perf -- --ignored --nocapture`).

use beskid_analysis::services::{PrepareOptions, resolve_input};
use beskid_pipeline::{TimedPipelineObserver, phases};
use beskid_queries::prepare_compilation_diagnostics_with_db;

use crate::projects::fixture_harness::{corelib_tests_project_root, with_project_test_env};

#[test]
#[ignore = "manual perf baseline; not a correctness gate"]
fn analysis_perf_ansi_style_chain_tests() {
    let root = corelib_tests_project_root();
    let entry_relative = "console/AnsiStyleChainTests.bd";
    let entry = root.join("src").join(entry_relative);
    if !entry.is_file() {
        return;
    }

    with_project_test_env(&root, || {
        let resolved =
            resolve_input(Some(&entry), Some(&root), None, None, false, false).expect("resolve");

        let timer = TimedPipelineObserver::new();
        beskid_queries::with_db(|db| {
            prepare_compilation_diagnostics_with_db(
                db,
                &resolved,
                PrepareOptions {
                    front_end: Default::default(),
                dependency_typing: beskid_analysis::services::DependencyTypingPolicy::FullClosure,
                },
                Some(&timer),
            )
        })
        .expect("prepare");

        let semantic = timer
            .phase_millis()
            .get(phases::SEMANTIC)
            .copied()
            .unwrap_or(0);
        let lower = timer
            .phase_millis()
            .get(phases::LOWER)
            .copied()
            .unwrap_or(0);
        let assemble = timer
            .phase_millis()
            .get(phases::PROGRAM_ASSEMBLE)
            .copied()
            .unwrap_or(0);
        eprintln!(
            "analysis_perf AnsiStyleChainTests: assemble={assemble}ms semantic={semantic}ms lower={lower}ms total={}ms",
            assemble + semantic + lower
        );
    });
}
