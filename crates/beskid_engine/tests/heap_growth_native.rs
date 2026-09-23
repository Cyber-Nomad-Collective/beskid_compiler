#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

//! Native (JIT, static AOT, native-kit shared library) evidence for the span/page heap redesign
//! (docs/superpowers/specs/2026-09-22-gc-span-heap-design.md), following the shape of
//! `fiber_value_transfer.rs`. Covers: growth past the 1 MiB initial region, allocation across
//! every size class, deep recursion holding thousands of concurrently rooted locals (the actual
//! regex/pest SIGILL shape), a fiber boundary interacting with span-queued marking, and cap
//! enforcement producing the typed `out_of_memory` trap instead of a crash.

use beskid_abi::{abi_v5::TRAP_DIAGNOSTIC_PREFIX, runtime_kit::BuildProfile};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded};
#[cfg(windows)]
use beskid_tests_support::native_harness::place_shared_runtime;
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{collections::HashSet, path::Path, process::Command, sync::Arc, time::Duration};

const ROUTE_LIMIT: Duration = Duration::from_secs(180);

fn heap_growth_assembly() -> Arc<ProgramAssembly> {
    heap_growth_assembly_with_fixture_source(None)
}

/// Same fixture assembly as [`heap_growth_assembly`], with the entry unit's source text replaced
/// by `fixture_source` when given (otherwise read from disk as usual). Used by the negative
/// legality-gate regression below, which needs the exact same corelib closure with one `use`
/// line removed rather than a from-scratch fixture.
fn heap_growth_assembly_with_fixture_source(fixture_source: Option<&str>) -> Arc<ProgramAssembly> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = root.ancestors().nth(2).expect("Engine crate must be nested under compiler/crates");
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    let application_root = root.join("tests/fixtures");
    let fixture_name = "heap_growth.bd";
    let source_path = application_root.join(fixture_name);
    let mut paths = vec![(source_path, fixture_name)];
    for (base, relative) in [
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
            let source = if logical_name == fixture_name && fixture_source.is_some() {
                fixture_source.expect("checked Some above").to_owned()
            } else {
                std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path:?}: {error}"))
            };
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
        SyntaxGenerationId(72),
    ))
}

/// Compiles `entry`, runs it under JIT asserting the exact `expected` result, then builds and
/// runs it under static AOT and the native kit through `heap_growth.c`, asserting only process
/// success (the fixture itself encodes correctness as a non-negative result, see
/// `fixtures/heap_growth.bd`).
fn run_heap_fixture(label: &str, entry: &str, expected: i64) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assembly = heap_growth_assembly();
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(&mut db, assembly, entry, target.clone(), isa.as_ref())
        .unwrap_or_else(|error| panic!("{label}: heap-growth source lowering failed: {error:?}"));
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    eprintln!(
        "{label} native kit: target={} source={}",
        kit.metadata.target.triple.as_str(),
        kit.metadata.source_hash
    );
    #[cfg(unix)]
    let shared_link_library = &kit.shared_library;
    #[cfg(windows)]
    let shared_link_library = {
        let library = kit.shared_import_library.as_ref().expect("Windows ABI-v5 kit requires a COFF import library");
        library
    };
    {
        let mut engine = beskid_engine::Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).unwrap();
        engine.compile_artifact(&lowered.artifact).expect("JIT heap-growth artifact");
        let entrypoint = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.unwrap();
        let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entrypoint) };
        let observed = run();
        assert_eq!(observed, expected, "{label}: JIT result mismatch");
        eprintln!("{label} JIT execution complete result={observed}");
    }
    let object_path = prefix.path().join(if cfg!(windows) { "heap_growth.obj" } else { "heap_growth.o" });
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object.compile_artifact_with_exports(&lowered.artifact, &HashSet::from([native_symbol.clone()]), None).unwrap();
    object.finalize_to_path(&object_path).unwrap();
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", shared_link_library)] {
        let directory = prefix.path().join(mode);
        std::fs::create_dir(&directory).unwrap();
        let executable = directory.join(executable_name("heap-growth"));
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
            .arg(library);
        #[cfg(unix)]
        cc.args(["-lpthread", "-lm"]);
        cc.arg("-o").arg(&executable).current_dir(&directory);
        let output = run_bounded(&format!("{label} {mode} build"), &mut cc, ROUTE_LIMIT);
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        #[cfg(windows)]
        if mode == "native-kit" {
            place_shared_runtime(&directory, &kit.shared_library);
        }
        let mut command = Command::new(&executable);
        #[cfg(unix)]
        command.env(
            if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
            kit.shared_library.parent().unwrap(),
        );
        let output = run_bounded(&format!("{label} {mode} execution"), &mut command, ROUTE_LIMIT);
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("{label} {mode} execution complete");
    }
}

#[test]
fn size_class_sweep_allocates_and_reuses_every_size_class() {
    // 8+16+...+4096 (doubling from 8, rounding each request up to its class) minus the trailing
    // "length - 1" term per class; the exact value only needs to be nonzero and stable, which the
    // JIT/AOT/native-kit agreement already proves — see `RunSizeClassSweep` in `heap_growth.bd`.
    // Computed once, offline, from the fixture's own arithmetic (13 classes rounding 8..4096).
    run_heap_fixture("size_class_sweep", "RunSizeClassSweep", size_class_sweep_expected());
}

#[test]
fn large_object_alone_allocates_and_reads_back_correctly() {
    // Isolates the dedicated-span path from every other allocation in this fixture, to narrow
    // down whether a failure elsewhere is specific to the large-object path.
    run_heap_fixture("large_object_only", "RunLargeObjectOnly", 11 + 5000);
}

fn size_class_sweep_expected() -> i64 {
    let sizes: [i64; 10] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
    sizes.iter().map(|size| ((size % 251) as i64) + (size - 1)).sum()
}

#[test]
fn growth_past_one_region_reports_multiple_regions_and_committed_bytes() {
    run_heap_fixture("growth_past_one_region", "RunGrowthPastOneRegion", 1024);
}

#[test]
fn span_reuse_minimal_bisection_aid() {
    // Temporary bisection aid, to be removed: phase 1 (fill+drop+collect) only, no cross-class
    // pressure phase. If this alone crashes, the bug is in a rooted local surviving its own
    // neighbor spans being freed, not in cross-class span reuse specifically.
    run_heap_fixture("span_reuse_minimal", "RunSpanReuseMinimal", 42);
}

#[test]
fn span_reuse_across_classes_after_collection_does_not_alias_objects() {
    // Regression for the stale `HEAP_CURRENT_SPAN_BY_CLASS` cache bug (see the doc comment on
    // `RunSpanReuseAcrossClasses` in heap_growth.bd): a span freed by `Sweep`'s empty-span path
    // must not still be trusted by a later allocation of the class it used to serve, once
    // `CarveSpan` has reused its page for a different class. Expected = survivor[0] (42) +
    // freshOne[0] (200) + freshTwo[0] (201) + freshThree[0] (202) + (Array.Len(freshOne) - 1)
    // (23) = 668, PROVIDED every object still reads back its own untouched content (no aliasing)
    // and `gc_heap_verify` passed; either failure mode drives the result negative instead.
    run_heap_fixture(
        "span_reuse_across_classes",
        "RunSpanReuseAcrossClasses",
        42 + 200 + 201 + 202 + 23,
    );
}

#[test]
fn deep_recursion_roots_thousands_of_locals_without_a_capacity_ceiling() {
    // `RecurseHoldingLocals(0, 6000)` returns 6000 plus the sum of `level[0] % 2` across every
    // frame; the exact checksum depends only on the deterministic fill byte
    // `remaining % 251`, computed once offline to keep this a real equality assertion rather than
    // a bare "did not crash" check.
    run_heap_fixture("deep_recursion", "RunDeepRecursion", deep_recursion_expected());
}

fn deep_recursion_expected() -> i64 {
    let mut total: i64 = 6000;
    for remaining in (1..=6000i64).rev() {
        let fill = (remaining % 251) as u8;
        total += i64::from(fill % 2);
    }
    total
}

#[test]
fn fiber_boundary_survives_collection_on_both_sides() {
    run_heap_fixture("fiber_heap_interaction", "RunFiberHeapInteraction", 14);
}

/// Regression for the reachability-scoped semantic legality gate
/// (`docs/superpowers/specs/2026-09-23-production-semantic-diagnostics-design.md`): before this
/// gate existed, `RunFiberHeapInteraction`'s `Result<u8[], FiberError> joined = ...;` without its
/// `use Concurrency.FiberError;` import failed only deep inside ISLE lowering, as an
/// unrelated-looking `MissingRuleOrFact` (`enum_match` could not build `FiberError`'s layout).
/// The gate must now reject it before specialization or ISLE ever runs, with a coded E1201 at the
/// `let`, and lowering must never reach `MissingRuleOrFact` for this case again.
#[test]
fn missing_fiber_error_import_is_rejected_by_the_legality_gate_not_a_missing_lowering_rule() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/heap_growth.bd"))
        .expect("read heap_growth.bd");
    let without_fiber_error_import =
        source.lines().filter(|line| *line != "use Concurrency.FiberError;").collect::<Vec<_>>().join("\n");
    assert_ne!(without_fiber_error_import, source, "the fixture must still carry the import to remove");

    let assembly = heap_growth_assembly_with_fixture_source(Some(&without_fiber_error_import));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();

    let result = lower_syntax_assembly_entrypoint(&mut db, assembly, "RunFiberHeapInteraction", target, isa.as_ref());
    let error = match result {
        Ok(_) => panic!("a `let` naming an unimported type must fail closed before lowering, not silently compile"),
        Err(error) => error,
    };
    let rendered = format!("{error:#}");

    assert!(rendered.contains("E1201"), "expected the legality gate's E1201 in: {rendered}");
    assert!(rendered.contains("FiberError"), "expected the unresolved name in: {rendered}");
    assert!(
        !rendered.contains("MissingRuleOrFact"),
        "the legality gate must stop this before ISLE ever reports a missing rule: {rendered}"
    );
}

/// Builds a standalone executable for `entry` (static AOT, linked against `heap_growth.c`) and
/// returns the temp directory (kept alive so the executable stays valid) and its path. Shared by
/// every test below that needs environment-variable control over the process (cap, fault
/// injection, stress mode) rather than the in-process JIT path `run_heap_fixture` uses.
fn build_heap_executable(label: &str, entry: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assembly = heap_growth_assembly();
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(&mut db, assembly, entry, target.clone(), isa.as_ref())
        .unwrap_or_else(|error| panic!("{label}: {entry} source lowering failed: {error:?}"));
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    let object_path = prefix.path().join(if cfg!(windows) { "fixture.obj" } else { "fixture.o" });
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object.compile_artifact_with_exports(&lowered.artifact, &HashSet::from([native_symbol.clone()]), None).unwrap();
    object.finalize_to_path(&object_path).unwrap();
    let executable = prefix.path().join(executable_name(&format!("heap-{label}")));
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
    let build_output = run_bounded(&format!("{label} build"), &mut cc, ROUTE_LIMIT);
    assert!(build_output.status.success(), "{}", String::from_utf8_lossy(&build_output.stderr));
    (prefix, executable)
}

#[test]
fn cap_hit_exits_through_the_out_of_memory_trap_instead_of_a_crash() {
    let (_prefix, executable) = build_heap_executable("cap-probe", "RunCapProbe");
    // `RunCapProbe` keeps 3 MiB reachable; a 2 MiB cap must trap before that finishes.
    let mut command = Command::new(&executable);
    command.env("BESKID_TEST_HEAP_CAP_BYTES", "2097152");
    let output = run_bounded("cap_probe execution", &mut command, ROUTE_LIMIT);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "cap probe must not succeed once the cap is set below the reachable set: {stderr}"
    );
    assert_eq!(output.status.code(), Some(101), "cap exhaustion must exit through the trap path, not a signal");
    assert!(stderr.contains(TRAP_DIAGNOSTIC_PREFIX), "{stderr}");
    assert!(stderr.contains("out_of_memory (5)"), "{stderr}");
    assert!(stderr.contains("cap=2097152"), "{stderr}");
}

#[test]
fn no_cap_configured_runs_the_cap_probe_to_completion() {
    run_heap_fixture("cap_probe_unbounded", "RunCapProbe", 768);
}

/// Fault-injection: `BESKID_TEST_FORCE_ROOT_STACK_FAILURE` makes the next root-block growth
/// behave as if `SystemAllocate` refused it, without needing to actually exhaust address space.
/// `RunDeepRecursion` roots far more than one 64-slot root-stack block can hold, so the forced
/// failure fires almost immediately and must produce the diagnosable trap (reason 4), not a
/// crash — this is the proof that a starved root stack fails exactly like a starved heap.
#[test]
fn root_stack_growth_failure_is_a_diagnosable_trap_not_a_crash() {
    let (_prefix, executable) = build_heap_executable("root-stack-fault", "RunDeepRecursion");
    let mut command = Command::new(&executable);
    command.env("BESKID_TEST_FORCE_ROOT_STACK_FAILURE", "1");
    let output = run_bounded("root_stack_fault execution", &mut command, ROUTE_LIMIT);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "forced root-stack failure must not let the program run to completion: {stderr}");
    assert_eq!(output.status.code(), Some(101), "root-stack exhaustion must exit through the trap path, not a signal");
    assert!(stderr.contains(TRAP_DIAGNOSTIC_PREFIX), "{stderr}");
    assert!(stderr.contains("out_of_memory (5)"), "{stderr}");
    assert!(stderr.contains("R4"), "diagnostic must carry failure reason 4 (root/handle stack growth failed): {stderr}");
}

/// Stress mode (`BESKID_TEST_STRESS_INTERVAL=1`, `Verify.bd`'s `gc_heap_set_stress_interval`)
/// collects at the START of every single `GcAlloc` call, before that call's own object is
/// carved — the strongest test the compiler's rooting is correct, since any GC-managed local or
/// temporary that is not yet rooted when it crosses an allocation that can collect will either
/// be swept (corrupting the result) or fail `gc_heap_verify`'s bitmap-consistency check. Both
/// fixtures assert their normal, non-stressed expected value: if compiler-emitted code ever
/// allocates before rooting a prior live value, this is expected to fail loudly here first.
#[test]
fn stress_mode_collecting_before_every_allocation_still_roots_deep_recursion_correctly() {
    let (_prefix, executable) = build_heap_executable("stress-recursion", "RunDeepRecursion");
    let mut command = Command::new(&executable);
    command.env("BESKID_TEST_STRESS_INTERVAL", "1");
    let output = run_bounded("stress_recursion execution", &mut command, ROUTE_LIMIT);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn stress_mode_collecting_before_every_allocation_still_roots_size_class_sweep_correctly() {
    let (_prefix, executable) = build_heap_executable("stress-size-classes", "RunSizeClassSweep");
    let mut command = Command::new(&executable);
    command.env("BESKID_TEST_STRESS_INTERVAL", "1");
    let output = run_bounded("stress_size_classes execution", &mut command, ROUTE_LIMIT);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
