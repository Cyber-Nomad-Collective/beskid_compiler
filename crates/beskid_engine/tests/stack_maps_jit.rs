#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

#[path = "support/shared_kit.rs"]
mod shared_kit;

use std::sync::Arc;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};

const SOURCE: &str = r#"
[Extern(Abi:"C")]
pub contract Gc {
    word gc_collect();
    word gc_phase();
    word gc_object_count();
    word gc_external_root_count();
}

type Node { i64 value }

word CollectAndCount() {
    Gc.gc_collect();
    if Gc.gc_phase() != word(0) { return word(1) << 63; }
    return Gc.gc_object_count();
}

i64 HoldThroughCollection(word objectCount, word rootCount) {
    Node object = Node { value: 42_i64 };
    mut i64 iteration = 0_i64;
    while iteration < 1_i64 {
        iteration = iteration + 1_i64;
    }
    if CollectAndCount() != objectCount + word(1) { return -1_i64; }
    word activeRoots = Gc.gc_external_root_count();
    if activeRoots == rootCount { return object.value; }
    return -2_i64;
}

i64 Main() {
    word objectCount = CollectAndCount();
    word externalRoots = Gc.gc_external_root_count();
    i64 result = HoldThroughCollection(objectCount, externalRoots);
    if CollectAndCount() != objectCount { return -3_i64; }
    if Gc.gc_external_root_count() != externalRoots { return -4_i64; }
    return result;
}
"#;

#[test]
fn engine_preserves_a_managed_root_through_nested_collection_then_reclaims_it() {
    let directory = tempfile::tempdir().expect("fixture directory");
    let source_path = directory.path().join("stack_maps_jit.bd");
    std::fs::write(&source_path, SOURCE).expect("write fixture");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), SOURCE).expect("parse fixture");
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.path().to_owned() },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit {
            origin_path: source_path.clone(),
            path: source_path,
            logical_name: "stack_maps_jit.bd".into(),
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
    let lowered = beskid_codegen::lower_syntax_assembly_entrypoint(
        &mut BeskidDatabase::default(),
        assembly,
        "Main",
        target.clone(),
        isa.as_ref(),
    )
    .expect("lower fixture");
    let register_root = cranelift_codegen::ir::ExternalName::testcase("gc_register_root");
    let explicit_roots = lowered
        .artifact
        .functions
        .iter()
        .any(|function| function.function.dfg.ext_funcs.values().any(|callee| callee.name == register_root));
    assert!(!explicit_roots, "qualification must run with stack-map roots only");
    assert!(
        lowered.artifact.extern_imports.iter().any(|import| import.symbol == "gc_collect_if_needed"),
        "the loop collection poll must resolve through the production engine"
    );
    let shared = shared_kit::debug();
    let mut engine =
        beskid_engine::Engine::with_runtime_kit(shared.prefix(), target, BuildProfile::Debug).expect("create Engine");
    engine.compile_artifact(&lowered.artifact).expect("compile fixture through Engine");
    let address = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.expect("finalized entry");
    let run: unsafe extern "C" fn() -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(unsafe { run() }, 42);
    eprintln!("managed root survived nested collection with zero external registrations");
}
