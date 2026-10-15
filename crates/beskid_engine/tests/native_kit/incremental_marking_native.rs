#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

//! Incremental marking evidence (v062-captures-and-move, tasks 9.1-9.5): the hybrid write
//! barrier (field and element overwrites, insertion half), allocation in black, phase order, the
//! chunked large-array trace with overwrites before and after the cursor, a suspended fiber that
//! holds the only reference, fibers that finish a cycle by yielding, the 100,000-operation stress
//! workload, the barrier-skip negative control and `gc_collect` during a cycle.
//!
//! `tests/fixtures/incremental_marking.bd` is lowered once per case, linked with the shared debug
//! kit through the `heap_growth.c` host (which runs the entry, requires a non-negative result and
//! a clean `gc_heap_verify`), and run as a child process.

use beskid_abi::abi_v5::{TRAP_DIAGNOSTIC_PREFIX, TRAP_EXIT_STATUS};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded};
use std::{collections::HashSet, path::Path, process::Command, sync::Arc, time::Duration};

use crate::shared_kit;

const ROUTE_LIMIT: Duration = Duration::from_secs(300);

fn assembly() -> Arc<ProgramAssembly> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = root.ancestors().nth(2).expect("Engine crate must be nested under compiler/crates");
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    let application_root = root.join("tests/fixtures");
    let fixture_name = "incremental_marking.bd";
    let mut paths = vec![(application_root.join(fixture_name), fixture_name)];
    for (base, relative) in [
        (&concurrency, "Concurrency.bd"),
        (&concurrency, "Concurrency/Fiber.bd"),
        (&concurrency, "Concurrency/FiberError.bd"),
        (&foundation, "Core/Disposable.bd"),
        (&foundation, "Core/Results/Results.bd"),
        (&foundation, "Core/Collections/Collections.bd"),
        (&foundation, "Core/Collections/Array.bd"),
        (&foundation, "Core/Collections/Array/ArrayIter.bd"),
    ] {
        paths.push((base.join(relative), relative));
    }
    let units = paths
        .into_iter()
        .map(|(path, logical_name)| {
            let source = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path:?}: {error}"));
            let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
            SourceUnit { origin_path: path.clone(), path, logical_name: logical_name.into(), source, program }
        })
        .collect();
    Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: application_root },
            dependencies: vec![
                RootEntry { dependency_name: Some("concurrency".into()), source_root: concurrency },
                RootEntry { dependency_name: Some("foundation".into()), source_root: foundation },
            ],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(73),
    ))
}

fn build(entry: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(&mut db, assembly(), entry, target, isa.as_ref())
        .unwrap_or_else(|error| panic!("{entry}: lowering failed: {error:?}"));
    let prefix = tempfile::tempdir().unwrap();
    let shared = shared_kit::debug();
    let kit = shared.kit();
    let object_path = prefix.path().join(if cfg!(windows) { "fixture.obj" } else { "fixture.o" });
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol(), &lowered.artifact().exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object.compile_artifact_with_exports(&lowered.artifact(), &HashSet::from([native_symbol.clone()]), None).unwrap();
    object.finalize_to_path(&object_path).unwrap();
    let executable = prefix.path().join(executable_name(&format!("incremental-{entry}")));
    let mut cc = native_c_compiler();
    cc.arg("-I")
        .arg(root.join("../beskid_abi/include"))
        .arg(format!(
            "-DHEAP_GROWTH_FIXTURE_SYMBOL=\"{}{}\"",
            if cfg!(target_os = "macos") { "_" } else { "" },
            native_symbol
        ))
        .arg(root.join("tests/fixtures/heap_growth.c"))
        .arg(&object_path)
        .arg(&kit.static_library);
    #[cfg(unix)]
    cc.args(["-lpthread", "-lm"]);
    cc.arg("-o").arg(&executable).current_dir(prefix.path());
    let output = run_bounded(&format!("{entry} build"), &mut cc, ROUTE_LIMIT);
    assert!(output.status.success(), "{entry}: {}", String::from_utf8_lossy(&output.stderr));
    let library_directory = kit.shared_library.parent().unwrap().to_path_buf();
    (prefix, executable, library_directory)
}

fn run(entry: &str, environment: &[(&str, &str)]) -> std::process::Output {
    let (_prefix, executable, _library_directory) = build(entry);
    let mut command = Command::new(&executable);
    command.env_remove("BESKID_GC_STRESS");
    for (key, value) in environment {
        command.env(key, value);
    }
    run_bounded(&format!("{entry} execution"), &mut command, ROUTE_LIMIT)
}

fn assert_passes(entry: &str, environment: &[(&str, &str)]) {
    let output = run(entry, environment);
    assert!(
        output.status.success(),
        "{entry} {environment:?}: exit {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// 9.1 lost-object cases, allocation in black, phase order, chunked trace, roots.
#[test]
fn incremental_cycles_keep_every_object_reachable_at_mark_start_or_allocated_during_it() {
    for entry in [
        "RunFieldOverwrite",
        "RunArrayElementOverwrite",
        "RunInsertionHalf",
        "RunAllocationInBlack",
        "RunPhaseOrder",
        "RunChunkedLargeArray",
        "RunSuspendedFiberRoot",
        "RunCollectDuringCycle",
    ] {
        assert_passes(entry, &[]);
        // The same case with every collection verified (v0.6.1 stress mode).
        assert_passes(entry, &[("BESKID_GC_STRESS", "verify")]);
    }
}

/// 9.2 negative control: a skipped barrier is found by the closure check at termination.
#[test]
fn incremental_stress_detects_a_skipped_barrier() {
    let output = run("RunBarrierSkipDetected", &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected = format!("{TRAP_DIAGNOSTIC_PREFIX}: runtime_internal_corruption (10): gc closure check");
    assert!(
        output.status.code() == Some(i32::from(TRAP_EXIT_STATUS)) && stderr.contains(&expected),
        "expected exit {TRAP_EXIT_STATUS} and `{expected}`; got {:?}, stderr `{}`",
        output.status.code(),
        stderr.trim()
    );
    assert_passes("RunFieldOverwrite", &[]);
}

/// 9.3 stress workload: identical results with a cycle step at every allocation.
#[test]
fn incremental_stress_workload_matches_the_unstressed_run() {
    assert_passes("RunStressWorkload", &[]);
}

/// 9.4 pacing: fibers that only yield finish a cycle; a 4 KiB step budget keeps the recorded step
/// bound; the 0.6.0 pacing scenarios stay in `heap_growth_native`.
#[test]
fn yielding_fibers_finish_a_cycle_without_allocating() {
    assert_passes("RunYieldingFibersFinishCycle", &[]);
}

/// 9.4 pacing: the mark trigger starts a cycle while the heap keeps growing, and garbage-only
/// allocation completes every cycle at the threshold without committing a second region.
#[test]
fn mark_trigger_does_not_block_growth_and_cycles_complete_before_growth() {
    assert_passes("RunTriggerWithoutBlockingGrowth", &[]);
    assert_passes("RunCompletionBeforeGrowth", &[]);
}

/// 9.4 cap pressure: a cap of 8 MiB, a 4 MiB graph dropped during a cycle and 4 MiB of new
/// reachable objects complete without exhaustion (two completed cycles at the cap).
#[test]
fn cap_pressure_completes_two_cycles_instead_of_trapping() {
    assert_passes("RunCapPressure", &[("BESKID_HEAP_MAX", "8m")]);
}

/// 9.5 pause bound: 64 MiB live and 128 MiB churn keep every paced step at or below
/// 256 KiB + 8 KiB. Wall times are recorded by the benchmark, not gated here.
#[test]
fn paced_mark_steps_stay_within_the_step_bound() {
    assert_passes("RunPauseBound", &[]);
}
