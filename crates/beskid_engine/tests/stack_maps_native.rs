#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

use std::{collections::HashSet, process::Command, sync::Arc, time::Duration};

use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded};
use cranelift_object::object::{self, Object, ObjectSection};

const SOURCE: &str = r#"
[Extern(Abi:"C")]
pub contract Probe {
    unit gc_collect();
    i64 host_checkpoint();
    i64 host_last_use(Node value);
    i64 host_pair(Node first, Node second);
    i64 collect_between_arguments();
}

type Node { i64 value }

[Export(Abi:"C", Symbol:"StackMapRead")]
pub i64 StackMapRead(Node node) { return node.value; }

[Export(Abi:"C", Symbol:"StackMapCallback")]
pub i64 StackMapCallback() {
    Probe.gc_collect();
    return 0_i64;
}

i64 NestedCollect() {
    return StackMapCallback();
}

Node SecondArgument() {
    i64 payload = Probe.collect_between_arguments();
    return Node { value: payload };
}

[Export(Abi:"C", Symbol:"RunStackMapFixture")]
pub i64 RunStackMapFixture() {
    Node value = Node { value: 42_i64 };
    u8[] retained = [13_u8];
    i64 first = StackMapRead(value);
    i64 nested = NestedCollect();
    i64 callback = Probe.host_checkpoint();
    i64 last = Probe.host_last_use(Node { value: 1_i64 });
    i64 pair = Probe.host_pair(Node { value: 2_i64 }, SecondArgument());
    return value.value + first + nested + callback + last + pair + i64(retained[0]) - 79_i64;
}
"#;

const HOST: &str = r#"
#include "beskid_runtime_abi_v5.h"
#include <assert.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
extern int64_t RunStackMapFixture(void);
extern int64_t StackMapCallback(void);
extern int64_t StackMapRead(void *node);
extern size_t gc_object_count(void);
static size_t baseline_objects;
#ifdef COLLECTION_WRAPPER
extern size_t __real_gc_collect(void);
static size_t collections;
#ifdef NEGATIVE_CONTROL
void __wrap_beskid_rt_v5_stack_map_push(void *frame, void *stack_pointer, void *table) {
    (void)frame; (void)stack_pointer; (void)table;
}
void __wrap_beskid_rt_v5_stack_map_pop(void *frame) { (void)frame; }
#endif
size_t __wrap_gc_collect(void) {
    collections++;
    size_t result = __real_gc_collect();
    if (gc_phase() != 0) {
        fprintf(stderr, "collection %zu did not return to idle (phase=%zu, result=%zu)\n",
                collections, gc_phase(), result);
        exit(72);
    }
    return result;
}
#endif
int64_t host_checkpoint(void) {
    int64_t result = StackMapCallback();
    assert(gc_phase() == 0);
    if (gc_object_count() <= baseline_objects) {
        fputs("stack-map live object missing\n", stderr);
        exit(71);
    }
    return result;
}
int64_t host_last_use(void *node) {
    gc_collect();
    assert(gc_phase() == 0);
    assert(gc_object_count() >= baseline_objects + 2);
    return StackMapRead(node);
}
int64_t collect_between_arguments(void) {
    gc_collect();
    assert(gc_phase() == 0);
    assert(gc_object_count() == baseline_objects + 3); /* value, retained, first argument */
    return 3;
}
int64_t host_pair(void *first, void *second) {
    const int samples = 1000;
    const int64_t start = clock_monotonic_nanos();
    for (int i = 0; i < samples; i++) gc_collect();
    const int64_t elapsed = clock_monotonic_nanos() - start;
    assert(gc_phase() == 0);
    assert(gc_object_count() >= baseline_objects + 4);
    printf("mean collection with live callback roots: %.0f ns (%d samples)\n", (double)elapsed / samples, samples);
    return StackMapRead(first) * 10 + StackMapRead(second);
}
int main(void) {
    _Alignas(8) unsigned char runtime[BESKID_RUNTIME_STATE_SIZE] = {0};
    assert(beskid_rt_v5_process_init(runtime) == runtime);
    gc_collect();
    assert(gc_phase() == 0);
    baseline_objects = gc_object_count();
    assert(RunStackMapFixture() == 42);
    gc_collect();
    assert(gc_phase() == 0);
    assert(gc_object_count() == baseline_objects);
    beskid_rt_v5_process_shutdown(runtime);
    return 0;
}
"#;

#[test]
fn managed_object_survives_nested_and_host_callback_collections_and_is_reclaimed() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let source_path = directory.path().join("stack_maps.bd");
    std::fs::write(&source_path, SOURCE).expect("write Beskid fixture");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), SOURCE).expect("parse fixture");
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.path().to_owned() },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit {
            origin_path: source_path.clone(),
            path: source_path,
            logical_name: "stack_maps.bd".into(),
            source: SOURCE.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(101),
    ));
    let target = beskid_engine::host_runtime_target().expect("host target");
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().expect("codegen settings");
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut database = BeskidDatabase::default();
    let lowered = beskid_codegen::lower_syntax_assembly_entrypoint(
        &mut database,
        assembly,
        "RunStackMapFixture",
        target,
        isa.as_ref(),
    )
    .expect("lower fixture");
    for function in &lowered.artifact.functions {
        let function = &function.function;
        for inst in function.layout.blocks().flat_map(|block| function.layout.block_insts(block)) {
            if let cranelift_codegen::ir::InstructionData::Call { func_ref, .. } = function.dfg.insts[inst] {
                for legacy in [
                    "gc_register_root",
                    "gc_unregister_root",
                    "beskid_rt_v5_array_allocate_rooted",
                    "beskid_rt_v5_array_grow_rooted",
                    "beskid_rt_v5_array_construction_finish",
                ] {
                    assert_ne!(
                        function.dfg.ext_funcs[func_ref].name,
                        cranelift_codegen::ir::ExternalName::testcase(legacy),
                        "map-only code must not call {legacy}"
                    );
                }
            }
        }
    }
    let symbols =
        HashSet::from(["RunStackMapFixture".to_owned(), "StackMapCallback".to_owned(), "StackMapRead".to_owned()]);
    let host_path = directory.path().join("host.c");
    std::fs::write(&host_path, HOST).expect("write host");
    for (profile, kit_profile) in [
        (beskid_aot::BuildProfile::Debug, RuntimeKitProfile::Debug),
        (beskid_aot::BuildProfile::Release, RuntimeKitProfile::Release),
    ] {
        let name = kit_profile.as_str();
        let kit = build_native_host(directory.path().join("kit"), kit_profile).expect("build matching runtime kit");
        let mut object =
            beskid_aot::object_module::BeskidObjectModule::new(None, profile).expect("create object module");
        object.compile_artifact_with_exports(&lowered.artifact, &symbols, None).expect("compile fixture");
        let object_path = directory.path().join(format!("stack_maps_{name}.o"));
        object.finalize_to_path(&object_path).expect("write fixture object");
        let bytes = std::fs::read(&object_path).expect("read fixture object");
        let object = object::File::parse(bytes.as_slice()).expect("parse fixture object");
        let metadata_bytes: u64 = object
            .sections()
            .filter(|section| matches!(section.name(), Ok(".data.rel.ro.beskid_gc" | "__beskid_gc" | ".rdata$GC")))
            .map(|section| section.size())
            .sum();
        assert!(metadata_bytes > 0, "fixture must contain stack maps");
        eprintln!("{name} stack-map metadata: {metadata_bytes} bytes");
        for negative in [false, true] {
            if negative && !cfg!(target_os = "linux") {
                continue;
            }
            let executable = directory.path().join(executable_name(&format!("stack-maps-{name}-{negative}")));
            let mut compiler = native_c_compiler();
            compiler
                .arg("-I")
                .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/include"))
                .arg(&host_path)
                .arg(&object_path)
                .arg(&kit.static_library);
            if cfg!(target_os = "linux") {
                compiler.args(["-DCOLLECTION_WRAPPER", "-Wl,--wrap=gc_collect"]);
            }
            if negative {
                compiler.args([
                    "-DNEGATIVE_CONTROL",
                    "-Wl,--wrap=beskid_rt_v5_stack_map_push",
                    "-Wl,--wrap=beskid_rt_v5_stack_map_pop",
                ]);
            }
            if cfg!(unix) {
                compiler.args(["-lpthread", "-lm"]);
            }
            compiler.arg("-o").arg(&executable);
            let output = run_bounded("stack-map fixture build", &mut compiler, Duration::from_secs(180));
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
            let output = run_bounded("stack-map fixture", &mut Command::new(&executable), Duration::from_secs(180));
            if negative {
                assert_eq!(output.status.code(), Some(71), "{name}: {}", String::from_utf8_lossy(&output.stderr));
                assert!(String::from_utf8_lossy(&output.stderr).contains("stack-map live object missing"));
            } else {
                assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
                eprint!("{name} {}", String::from_utf8_lossy(&output.stdout));
            }
        }
    }
}
