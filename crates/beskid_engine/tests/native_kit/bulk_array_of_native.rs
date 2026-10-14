#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

//! Generic `bulk T[]` calls (`Array.Of<T>`) end to end on the canonical runtime kit: one-argument
//! and multi-argument calls, managed elements across collections, a call inside a generic item and
//! the resolver-spike frontier loop that crashed when a one-argument `Array.Of` passed its scalar
//! as the array handle.

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use std::{path::Path, sync::Arc};
use crate::shared_kit;

fn bulk_array_of_assembly() -> Arc<ProgramAssembly> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = root.ancestors().nth(2).expect("Engine crate must be nested under compiler/crates");
    let foundation = compiler.join("corelib/packages/foundation/src");
    let application_root = root.join("tests/fixtures");
    let fixture_name = "bulk_array_of.bd";
    let mut paths = vec![(application_root.join(fixture_name), fixture_name)];
    for relative in ["Core/Collections/Collections.bd", "Core/Collections/Array.bd", "Core/Collections/Array/ArrayIter.bd"] {
        paths.push((foundation.join(relative), relative));
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
            dependencies: vec![RootEntry { dependency_name: Some("foundation".into()), source_root: foundation }],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(87),
    ))
}

#[test]
fn generic_bulk_array_of_calls_pack_rooted_arrays() {
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = beskid_codegen::lower_syntax_assembly_entrypoint(
        &mut db,
        bulk_array_of_assembly(),
        "RunBulkArrayOf",
        target.clone(),
        isa.as_ref(),
    )
    .expect("generic bulk calls must lower through the bulk packing path");
    let shared = shared_kit::debug();
    let mut engine = beskid_engine::Engine::with_runtime_kit(shared.prefix(), target, BuildProfile::Debug).unwrap();
    engine.compile_artifact(&lowered.artifact()).expect("compile bulk Array.Of artifact");
    let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol()) }.unwrap();
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
    assert_eq!(run(), 42, "the fixture returns a distinct failure code for each bulk call shape");
}
