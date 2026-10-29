#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

//! Generic functions over array parameters, end to end (beskid-lings milestone 3): the fixture is
//! lowered through the query layer and ISLE and runs on the canonical runtime kit. Equality on the
//! loop variable of `for value in values` compares string contents in the `T = string`
//! specialization, and `FirstOr([7, 8], 0)` infers `T` from the bare integers of its array literal.

use crate::shared_kit;
use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use std::{path::Path, sync::Arc};

#[test]
fn generic_for_variables_compare_by_value_and_bare_integer_arrays_bind_t() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let path = root.join("generic_for_values.bd");
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
            logical_name: "generic_for_values.bd".into(),
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(167),
    ));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = beskid_codegen::target_isa::host_isa_builder()
        .unwrap()
        .finish(cranelift_codegen::settings::Flags::new(settings))
        .unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = beskid_codegen::lower_syntax_assembly_entrypoint(
        &mut db,
        assembly,
        "RunGenericForValues",
        target.clone(),
        isa.as_ref(),
    )
    .expect("generic array functions must lower through source facts");
    let shared = shared_kit::debug();
    let mut engine = beskid_engine::Engine::with_runtime_kit(shared.prefix(), target, BuildProfile::Debug).unwrap();
    engine.compile_artifact(&lowered.artifact()).expect("compile generic-for-values artifact");
    let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol()) }.unwrap();
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
    assert_eq!(run(), 42, "the fixture returns a distinct failure code for each generic call");
}
