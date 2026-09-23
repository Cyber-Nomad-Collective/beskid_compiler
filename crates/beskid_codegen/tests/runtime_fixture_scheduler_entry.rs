//! Regression test for the scheduler-entry reachability gap in general entrypoint lowering.
//!
//! `lower_syntax_assembly_entrypoint` (the general path used by test/AOT/JIT compilation, not
//! only `lower_canonical_runtime_prepared_syntax`'s canonical-runtime-corpus build) must resolve
//! the *complete* `SCHEDULER_ENTRY_HELPERS` set once a runtime-fixture-capability compilation
//! needs any one of them — `FiberDone`/`SchedulerSetCurrentFiber`/etc. are invoked only by
//! compiler-generated scheduler entry/return trampolines, never by a direct source call, so
//! ordinary call-graph reachability from the entrypoint can pull in some of them while missing
//! others. Before the fix this failed closed with `canonical FiberDone item unavailable` /
//! `canonical SchedulerSetCurrentFiber item unavailable` from
//! `crates/beskid_codegen/src/module_emission/orchestration.rs`.

use std::sync::Arc;

use beskid_abi::abi_v5::TargetMetadata;
use beskid_abi::runtime_source::runtime_fixture_project_root;
use beskid_analysis::projects::{
    ProgramAssembly, assemble_program_with_materializer, assembly_options_for_plan, build_compile_plan, plan_entry_path,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_queries::BeskidDatabase;
use cranelift_codegen::isa;
use cranelift_codegen::settings;

/// Assemble one of the compiler-embedded canonical runtime fixture targets, exactly as
/// `crates/beskid_queries/tests/runtime_fixture.rs` does for its own coverage of the fixture
/// authority proof.
fn fixture(target: &str) -> ProgramAssembly {
    let manifest = runtime_fixture_project_root().join("runtime_semantics.bproj");
    let plan = build_compile_plan(&manifest, Some(target)).expect("real fixture plan");
    let entry = plan_entry_path(&plan, &plan.source_root);
    let source = std::fs::read_to_string(&entry).expect("fixture source");
    assemble_program_with_materializer(&plan, None, &entry, Some(&source), &assembly_options_for_plan(&plan), None, None)
        .expect("exact runtime and real Corelib assembly")
}

fn x86_64_target_and_isa() -> (TargetMetadata, Arc<dyn cranelift_codegen::isa::TargetIsa>) {
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux x86_64 ABI target");
    let flags = beskid_codegen::cranelift_host::production_isa_settings_builder().expect("production ISA settings");
    let isa = isa::lookup_by_name("x86_64").expect("x86 ISA").finish(settings::Flags::new(flags)).expect("finish ISA");
    (target, isa)
}

/// `ExternalWorkTests.bd`'s `cancelled_wait_retains_host_work_until_host_exit` reaches
/// `Core.FiberAlloc`, which requires a spawn/context trampoline, which in turn requires the
/// complete `SCHEDULER_ENTRY_HELPERS` set — the exact shape that used to fail closed.
#[test]
fn external_work_tests_fixture_entrypoint_resolves_the_full_scheduler_entry_helper_set() {
    let assembly = Arc::new(fixture("ExternalWorkTests"));
    let (target, isa) = x86_64_target_and_isa();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(
        &mut db,
        assembly,
        "cancelled_wait_retains_host_work_until_host_exit",
        target,
        isa.as_ref(),
    );
    assert!(
        lowered.is_ok(),
        "a runtime-fixture entrypoint must resolve every SCHEDULER_ENTRY_HELPERS item once any one \
         of them is needed, not only the ones its own call graph happens to reach directly: {:?}",
        lowered.err()
    );
}

/// `LifecycleTests.bd`'s `lifecycle_uses_manifest_state_generation_and_48_byte_tls` is a second,
/// independent fixture target exercising the same reachability gap through a different call path
/// (`Lifecycle.ProcessInit`, not `Core.FiberAlloc`).
#[test]
fn lifecycle_tests_fixture_entrypoint_resolves_the_full_scheduler_entry_helper_set() {
    let assembly = Arc::new(fixture("LifecycleTests"));
    let (target, isa) = x86_64_target_and_isa();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(
        &mut db,
        assembly,
        "lifecycle_uses_manifest_state_generation_and_48_byte_tls",
        target,
        isa.as_ref(),
    );
    assert!(
        lowered.is_ok(),
        "a runtime-fixture entrypoint must resolve every SCHEDULER_ENTRY_HELPERS item once any one \
         of them is needed, not only the ones its own call graph happens to reach directly: {:?}",
        lowered.err()
    );
}
