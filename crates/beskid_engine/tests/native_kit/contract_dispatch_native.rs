#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

//! Runtime contract dispatch (v0.6.7), end to end on the canonical runtime kit: the fixture's
//! contract values dispatch through their witness tables under the JIT, and the same artifact
//! (witness tables, box descriptors and thunks included) compiles to an AOT object.

use crate::shared_kit;
use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use std::{path::Path, sync::Arc};

fn lowered() -> beskid_codegen::PreparedSyntaxEntrypoint {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let path = root.join("contract_dispatch.bd");
    let source = std::fs::read_to_string(&path).unwrap();
    let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit {
            origin_path: path.clone(),
            path,
            logical_name: "contract_dispatch.bd".into(),
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(91),
    ));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = beskid_codegen::target_isa::host_isa_builder()
        .unwrap()
        .finish(cranelift_codegen::settings::Flags::new(settings))
        .unwrap();
    let mut db = BeskidDatabase::default();
    beskid_codegen::lower_syntax_assembly_entrypoint(&mut db, assembly, "RunContractDispatch", target, isa.as_ref())
        .expect("contract values must lower through source facts")
}

#[test]
fn contract_values_dispatch_through_witness_tables_under_the_jit() {
    let lowered = lowered();
    let artifact = lowered.artifact();
    assert!(artifact.contract_boxes_required, "the fixture boxes contract values");
    assert!(artifact.witness_tables.len() >= 3, "Hello, Bye and i32 each have a table: {:?}", artifact.witness_tables);
    let target = beskid_engine::host_runtime_target().unwrap();
    let shared = shared_kit::debug();
    let mut engine = beskid_engine::Engine::with_runtime_kit(shared.prefix(), target, BuildProfile::Debug).unwrap();
    engine.compile_artifact(artifact).expect("compile contract dispatch artifact");
    let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol()) }.unwrap();
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
    assert_eq!(run(), 42, "the fixture returns a distinct failure code for each dispatch shape");
}

#[test]
fn contract_values_compile_to_an_aot_object() {
    let lowered = lowered();
    let symbol = beskid_codegen::object_link_symbol(&lowered.symbol(), &lowered.artifact().exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object
        .compile_artifact_with_exports(lowered.artifact(), &std::collections::HashSet::from([symbol]), None)
        .expect("witness tables and thunks compile into an object");
    let prefix = tempfile::tempdir().unwrap();
    object.finalize_to_path(&prefix.path().join("contract_dispatch.o")).expect("object written");
}
