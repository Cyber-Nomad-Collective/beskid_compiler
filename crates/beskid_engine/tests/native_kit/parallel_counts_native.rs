#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

//! Isolate, port and ORCA count evidence (v063 4.x, v064 6.1, v065 6.1): two isolates with
//! adopted results, port FIFO and close-then-drain, Shared values by pointer (the echoed address
//! is the original one), a Shared value that outlives its sender, a Moved graph handed over by
//! pointer while its owner churns and collects (owner shallow marking, receiver tracing through
//! foreign mutable objects), a failed owner retained and adopted because another isolate holds a
//! stake, and undelivered stakes released with the scope's ports. Each case also runs with every
//! collection verified (`BESKID_GC_STRESS=verify`) and with `BESKID_ISOLATES_MAX=1` (serial
//! elision: the same results without a second heap).
//!
//! `tests/fixtures/parallel_counts.bd` is lowered once per case against the Core.Parallel
//! Corelib sources, linked with the shared debug kit through the `heap_growth.c` host (which runs
//! the entry, requires a non-negative result and a clean `gc_heap_verify`), and run as a child
//! process.

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

fn corelib_units(base: &Path, directory: &Path, units: &mut Vec<(std::path::PathBuf, String)>) {
    let mut entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {directory:?}: {error}"))
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            corelib_units(base, &path, units);
        } else if path.extension().is_some_and(|extension| extension == "bd") {
            let logical = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            units.push((path, logical));
        }
    }
}

fn assembly() -> Arc<ProgramAssembly> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = root.ancestors().nth(2).expect("Engine crate must be nested under compiler/crates");
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    let application_root = root.join("tests/fixtures");
    let fixture_name = "parallel_counts.bd";
    let mut paths = vec![(application_root.join(fixture_name), fixture_name.to_owned())];
    // Core.Parallel reaches most of both packages; import-closure discovery keeps what is used.
    corelib_units(&concurrency, &concurrency, &mut paths);
    corelib_units(&foundation, &foundation, &mut paths);
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
        SyntaxGenerationId(74),
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
    let executable = prefix.path().join(executable_name(&format!("parallel-{entry}")));
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
    command.env_remove("BESKID_ISOLATES_MAX");
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

fn assert_passes_everywhere(entry: &str) {
    assert_passes(entry, &[]);
    assert_passes(entry, &[("BESKID_GC_STRESS", "verify")]);
    assert_passes(entry, &[("BESKID_ISOLATES_MAX", "1")]);
}

/// v0.6.3: two isolate heaps run and their results are adopted at join.
#[test]
fn two_isolates_join_with_adopted_results() {
    assert_passes_everywhere("RunTwoIsolates");
}

/// v0.6.4 2.1-2.3: FIFO under back-pressure (capacity 4), close then drain.
#[test]
fn port_is_fifo_under_back_pressure_and_drains_after_close() {
    assert_passes_everywhere("RunPortFifo");
}

/// v0.6.4 4.5 and 3.1-3.2: a Shared value crosses by pointer; the INC, the receiver's DEC and the
/// own-object receive balance, and the value stays valid after its sender exits.
#[test]
fn shared_values_cross_ports_by_pointer_and_outlive_their_sender() {
    assert_passes_everywhere("RunSharedByPointer");
    assert_passes_everywhere("RunSharedOutlivesSender");
}

/// v0.6.5 2.1-2.3 and 3.1-3.2: a Moved graph is handed over by pointer; the owner shallow-marks
/// it while it churns, the receiver traces through it and stores its own objects inside.
#[test]
fn moved_graphs_are_handed_over_by_pointer() {
    assert_passes_everywhere("RunMovedHandOver");
}

/// v0.6.4 3.3 and v0.6.5 3.3: a failed owner whose object another isolate holds is adopted, not
/// discarded; undelivered stakes are released with the scope's ports.
#[test]
fn counted_heaps_are_retained_and_undelivered_stakes_released() {
    assert_passes_everywhere("RunFailedOwnerRetained");
    assert_passes_everywhere("RunUndeliveredReleased");
}

/// v0.6.3: one `Spawn` on a real isolate returns its value through Join (and through serial
/// elision at a bound of 1).
#[test]
fn spawned_value_returns_through_join() {
    for bound in ["2", "8", "1"] {
        let output = run("RunSpawnValue", &[("BESKID_ISOLATES_MAX", bound)]);
        assert!(
            output.status.success(),
            "RunSpawnValue bound {bound}: exit {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// v0.6.3: a body that returns `Result::Error` through `Spawn` is a task value, also when every
/// allocation collects.
#[test]
fn tolerated_error_is_a_task_value() {
    for environment in [
        &[("BESKID_ISOLATES_MAX", "1"), ("BESKID_GC_STRESS", "alloc:1,verify")][..],
        &[("BESKID_ISOLATES_MAX", "2"), ("BESKID_GC_STRESS", "alloc:1,verify")][..],
        &[("BESKID_ISOLATES_MAX", "2")][..],
    ] {
        let output = run("RunToleratedValue", environment);
        assert!(
            output.status.success(),
            "RunToleratedValue {environment:?}: exit {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
