#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

#[path = "support/shared_kit.rs"]
mod shared_kit;

use std::{collections::HashSet, process::Command, sync::Arc, time::Duration};

use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded};

const SOURCE: &str = r#"
[Extern(Abi:"C")]
pub contract Host {
    i64 host_role();
    pointer host_group();
    i32 wait_group_wait(pointer group);
    unit host_drive();
}

type Node { i64 value }

[Export(Abi:"C", Symbol:"StackMapRun")]
pub i64 StackMapRun() {
    if Host.host_role() == 1_i64 {
        Node parked = Node { value: 42_i64 };
        i32 status = Host.wait_group_wait(Host.host_group());
        return parked.value + i64(status);
    }
    Node scheduler = Node { value: 17_i64 };
    Host.host_drive();
    return scheduler.value;
}
"#;

const HOST: &str = r#"
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>

extern int64_t StackMapRun(void);
static int64_t role;
static int64_t parked_result;
static size_t baseline;
static void *group;
static void *dummy;

int64_t host_role(void) { return role; }
void *host_group(void) { return group; }

static void *park(void *argument) {
    role = 1;
    parked_result = StackMapRun();
    role = 0;
    return argument;
}

static void *collect(void *argument) {
    const size_t before = gc_object_count();
    assert(before == baseline + 3); /* dummy plus the scheduler's and the parked fiber's objects */
    gc_collect();
    assert(gc_phase() == 0);
    assert(gc_object_count() == before);
    wait_group_done(group);
    return argument;
}

void host_drive(void) {
    int64_t parked = fiber_spawn((void *)park, dummy);
    int64_t collector = fiber_spawn((void *)collect, dummy);
    assert(parked >= 0 && collector >= 0);
    assert(fiber_join_status(parked) == 0);
    assert(fiber_join_status(collector) == 0);
    assert(parked_result == 42);
    fiber_detach(parked);
    fiber_detach(collector);
}

int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    gc_collect();
    assert(gc_phase() == 0);
    baseline = gc_object_count();
    uintptr_t descriptor[] = {24, 8, 0, 0, 0};
    uintptr_t request[] = {24, 8, (uintptr_t)descriptor};
    dummy = beskid_rt_v5_managed_object_allocate(request);
    assert(dummy && gc_register_root(&dummy));
    group = wait_group_create();
    assert(group);
    wait_group_add(group, 1);

    gc_heap_set_stress_interval(1);
    assert(StackMapRun() == 17);
    gc_heap_set_stress_interval(0);
    gc_unregister_root(&dummy);
    gc_collect();
    assert(gc_phase() == 0 && gc_object_count() == baseline);
    assert(gc_heap_verify());
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
"#;

#[test]
fn aot_stack_maps_keep_scheduler_and_parked_fiber_objects_alive() {
    let directory = tempfile::tempdir().expect("fiber fixture directory");
    let source_path = directory.path().join("stack_maps_parked.bd");
    std::fs::write(&source_path, SOURCE).expect("write fiber fixture");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), SOURCE).expect("parse fixture");
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.path().to_owned() },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit {
            origin_path: source_path.clone(),
            path: source_path,
            logical_name: "stack_maps_parked.bd".into(),
            source: SOURCE.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(102),
    ));
    let target = beskid_engine::host_runtime_target().expect("host target");
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().expect("codegen settings");
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut database = BeskidDatabase::default();
    let lowered =
        beskid_codegen::lower_syntax_assembly_entrypoint(&mut database, assembly, "StackMapRun", target, isa.as_ref())
            .expect("lower fixture");
    assert!(
        !beskid_codegen::referenced_extern_imports(&lowered.artifact).iter().any(|import| {
            matches!(import.symbol.as_str(), "gc_register_root" | "gc_unregister_root" | "gc_root_handle")
        }),
        "the fixture must depend on stack maps for its managed locals"
    );
    let symbols = HashSet::from(["StackMapRun".to_owned()]);
    let host_path = directory.path().join("host.c");
    std::fs::write(&host_path, HOST).expect("write host");
    let shared = shared_kit::debug();
    for (name, profile) in [("debug", beskid_aot::BuildProfile::Debug), ("release", beskid_aot::BuildProfile::Release)]
    {
        let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, profile).expect("create object");
        object.compile_artifact_with_exports(&lowered.artifact, &symbols, None).expect("compile fiber fixture");
        let object_path = directory.path().join(format!("stack_maps_parked_{name}.o"));
        object.finalize_to_path(&object_path).expect("write fixture object");
        let executable = directory.path().join(executable_name(&format!("stack-maps-parked-{name}")));
        let mut compiler = native_c_compiler();
        compiler
            .arg("-I")
            .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/include"))
            .arg(&host_path)
            .arg(object_path)
            .arg(&shared.kit().static_library);
        if cfg!(unix) {
            compiler.args(["-lpthread", "-lm"]);
        }
        compiler.arg("-o").arg(&executable);
        let output = run_bounded("parked stack-map fixture build", &mut compiler, Duration::from_secs(180));
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        let output = run_bounded("parked stack-map fixture", &mut Command::new(executable), Duration::from_secs(180));
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
