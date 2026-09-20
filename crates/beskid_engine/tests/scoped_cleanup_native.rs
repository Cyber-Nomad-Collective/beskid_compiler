#![cfg(unix)]

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{path::Path, sync::Arc};

#[test]
fn scoped_cleanup_runs_once_in_reverse_order_on_every_structured_exit() {
    for fixture in
        ["scoped_cleanup_unit.bd", "scoped_cleanup_scalar.bd", "scoped_cleanup_string.bd", "scoped_cleanup.bd"]
    {
        run_cleanup_fixture(fixture);
    }
}

#[test]
fn scoped_cleanup_preserves_constructor_and_argument_roots() {
    run_cleanup_fixture("scoped_cleanup_constructors.bd");
}

fn run_cleanup_fixture(fixture: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let path = root.join(fixture);
    let source = std::fs::read_to_string(&path).unwrap();
    let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit { path, logical_name: "scoped_cleanup.bd".into(), source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        SyntaxGenerationId(85),
    ));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = beskid_codegen::lower_syntax_assembly_entrypoint(
        &mut db,
        assembly,
        "RunCleanupFixture",
        target.clone(),
        isa.as_ref(),
    )
    .expect("scoped cleanup must lower through source facts");
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    let mut engine = beskid_engine::Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).unwrap();
    engine.compile_artifact(&lowered.artifact).expect("compile scoped cleanup artifact");
    let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.unwrap();
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
    assert_eq!(run(), 42, "fixture returns a distinct failure code for each cleanup obligation");
    drop(engine);
    let object_path = prefix.path().join("cleanup.o");
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object
        .compile_artifact_with_exports(
            &lowered.artifact,
            &std::collections::HashSet::from([native_symbol.clone()]),
            None,
        )
        .unwrap();
    object.finalize_to_path(&object_path).unwrap();
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
        let executable = prefix.path().join(mode);
        let output = std::process::Command::new("cc")
            .arg("-std=c11")
            .arg("-I")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/include"))
            .arg(format!(
                "-DCLEANUP_FIXTURE_SYMBOL=\"{}{}\"",
                if cfg!(target_os = "macos") { "_" } else { "" },
                native_symbol
            ))
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scoped_cleanup.c"))
            .arg(&object_path)
            .arg(library)
            .args(["-lpthread", "-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        let output = std::process::Command::new(executable)
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
