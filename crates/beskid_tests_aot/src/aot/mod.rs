//! End-to-end AOT tests: codegen artifact → object / link, entrypoints, runtime strategies.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source};
pub(super) use beskid_aot::{
    AotBuildRequest, AotError, BuildOutputKind, ProjectTargetKind, build, default_output_kind, resolve_entrypoint,
};
use beskid_queries::compile_front_end_from_resolved_input;

mod defaults;
mod entrypoint;
mod object_build;

/// Isolated temp directory for AOT outputs (distinct prefix from `test_harness::temp_case_dir`).
fn temp_case_dir(name: &str) -> PathBuf {
    static NEXT_CASE_ID: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time ok").as_nanos();
    let case_id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("beskid_aot_tests_{name}_{}_{}_{}", std::process::id(), nanos, case_id));
    std::fs::create_dir(&dir).expect("create unique temp dir");
    dir
}

#[test]
fn simultaneous_temp_cases_keep_distinct_source_paths() {
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};

    const THREADS: usize = 16;
    const ROUNDS: usize = 100;
    let barrier = Arc::new(Barrier::new(THREADS));
    let paths = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..THREADS)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let mut paths = Vec::with_capacity(ROUNDS);
                    for _ in 0..ROUNDS {
                        barrier.wait();
                        paths.push(temp_case_dir("simultaneous_source"));
                    }
                    paths
                })
            })
            .collect();
        workers.into_iter().flat_map(|worker| worker.join().expect("temp case worker")).collect::<Vec<_>>()
    });
    let unique: HashSet<_> = paths.iter().collect();
    for path in unique.iter() {
        std::fs::remove_dir_all(path).expect("remove temp case");
    }
    assert_eq!(unique.len(), THREADS * ROUNDS, "simultaneous tests must never share a source path");
}

/// Minimal valid program source for default AOT samples.
fn sample_program() -> &'static str {
    "unit Main() { }"
}

/// Prepare `sample_program` once, then lower it through the production syntax → CodegenInput → ISLE boundary.
fn lower_sample_artifact() -> beskid_codegen::CodegenArtifact {
    let source = sample_program();
    let dir = temp_case_dir("prepared_syntax_sample");
    let path = dir.join("Main.bd");
    std::fs::write(&path, source).expect("write sample source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved = resolved_input_from_plan(path, source.to_owned(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .expect("prepare sample frontend");
    let aot_target = beskid_aot::target::detect_target(None).expect("detect host AOT target");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == aot_target.triple)
        .expect("host AOT target must have an ABI-v5 metadata contract");
    let artifact =
        beskid_aot::lower_prepared_syntax_entrypoint(&front, "Main", target).expect("lower sample through syntax ISLE");
    let _ = std::fs::remove_dir_all(dir);
    artifact
}
