#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
#[cfg(windows)]
use beskid_tests_support::native_harness::place_shared_runtime;
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{path::Path, process::Command, sync::Arc, time::Duration};

const ROUTE_LIMIT: Duration = Duration::from_secs(60);

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
    let object_path = prefix.path().join(if cfg!(windows) { "cleanup.obj" } else { "cleanup.o" });
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
    let run_native_routes = || {
        #[cfg(unix)]
        let shared_link_library = &kit.shared_library;
        #[cfg(windows)]
        let shared_link_library =
            kit.shared_import_library.as_ref().expect("Windows ABI-v5 kit must contain a COFF import library");
        for (mode, library) in [("aot", &kit.static_library), ("native-kit", shared_link_library)] {
            let directory = prefix.path().join(mode);
            std::fs::create_dir(&directory).unwrap();
            let executable = directory.join(executable_name("scoped-cleanup"));
            let mut command = native_c_compiler();
            command
                .args(["-Wall", "-Wextra", "-Werror"])
                .arg("-I")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_abi/include"))
                .arg(format!(
                    "-DCLEANUP_FIXTURE_SYMBOL=\"{}{}\"",
                    if cfg!(target_os = "macos") { "_" } else { "" },
                    native_symbol
                ))
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scoped_cleanup.c"))
                .arg(&object_path)
                .arg(library);
            #[cfg(unix)]
            command.args(["-lpthread", "-lm"]);
            command.arg("-o").arg(&executable).current_dir(&directory);
            let output = run_bounded(&format!("{mode}: compile"), &mut command, ROUTE_LIMIT);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            #[cfg(windows)]
            if mode == "native-kit" {
                place_shared_runtime(&directory, &kit.shared_library);
            }
            let mut command = Command::new(&executable);
            #[cfg(unix)]
            if mode == "native-kit" {
                command.env(
                    if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                    kit.shared_library.parent().unwrap(),
                );
            }
            let output = run_bounded(&format!("{mode}: execute"), &mut command, ROUTE_LIMIT);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        }
    };
    #[cfg(windows)]
    run_native_routes();
    let mut engine = beskid_engine::Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).unwrap();
    engine.compile_artifact(&lowered.artifact).expect("compile scoped cleanup artifact");
    let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.unwrap();
    let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
    assert_eq!(run(), 42, "fixture returns a distinct failure code for each cleanup obligation");
    #[cfg(unix)]
    run_native_routes();
}
