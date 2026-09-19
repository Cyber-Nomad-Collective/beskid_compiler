#![cfg(unix)]

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
};
use beskid_codegen::lower_syntax_assembly_entrypoint;
use beskid_queries::{BeskidDatabase, SyntaxGenerationId};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{collections::HashSet, path::Path, process::Command, sync::Arc};

#[test]
fn source_fiber_values_survive_collection_in_jit_aot_and_native_kit() {
    source_transfer_fixture("fiber", "RunFiberFixture", 126);
}

#[test]
fn source_channel_values_survive_collection_in_jit_aot_and_native_kit() {
    source_transfer_fixture("channel", "RunChannelFixture", 126);
}

#[test]
fn source_channel_claim_cancellation_cleanup_preserves_typed_value() {
    source_transfer_fixture("channel_receipt", "RunChannelReceiptFixture", 126);
}

#[test]
fn source_external_cancellation_is_sticky_across_new_waits_but_not_fiber_reuse() {
    source_transfer_fixture("external_cancel", "RunExternalCancellationFixture", 126);
}

fn source_transfer_fixture(kind: &str, entry: &str, expected: i64) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiler = root.join("../..").canonicalize().unwrap();
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    let application_root = root.join("tests/fixtures");
    let fixture_name = format!("{kind}_value_transfer.bd");
    let source_path = application_root.join(&fixture_name);
    let mut paths = vec![(source_path, fixture_name.as_str())];
    for (base, relative) in [
        (&concurrency, "Concurrency/Fiber.bd"),
        (&concurrency, "Concurrency/FiberError.bd"),
        (&concurrency, "Concurrency/Channel.bd"),
        (&concurrency, "Concurrency/ChannelError.bd"),
        (&concurrency, "Concurrency/ChannelOptions.bd"),
        (&concurrency, "Concurrency/Status.bd"),
        (&concurrency, "Concurrency/TryResult.bd"),
        (&concurrency, "Concurrency/Hub.bd"),
        (&concurrency, "Concurrency/HubError.bd"),
        (&concurrency, "Concurrency/HubReceiveResult.bd"),
        (&foundation, "Core/Results/Results.bd"),
    ] {
        paths.push((base.join(relative), relative));
    }
    let units = paths
        .into_iter()
        .map(|(path, logical_name)| {
            let source = std::fs::read_to_string(&path).unwrap();
            let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
            SourceUnit { path, logical_name: logical_name.into(), source, program }
        })
        .collect();
    let assembly = Arc::new(ProgramAssembly::new(
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
        SyntaxGenerationId(71),
    ));
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(&mut db, assembly, entry, target.clone(), isa.as_ref())
        .expect("typed Fiber source lowering");
    eprintln!("Fiber source lowering complete");
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    eprintln!("Fiber native kit complete");
    {
        let mut engine = beskid_engine::Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).unwrap();
        engine.compile_artifact(&lowered.artifact).expect("JIT typed Fiber artifact");
        let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.unwrap();
        let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
        eprintln!("Fiber JIT execution start");
        assert_eq!(run(), expected);
        eprintln!("Fiber JIT execution complete");
    }
    let object_path = prefix.path().join("fiber.o");
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object.compile_artifact_with_exports(&lowered.artifact, &HashSet::from([native_symbol.clone()]), None).unwrap();
    object.finalize_to_path(&object_path).unwrap();
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", &kit.shared_library)] {
        let executable = prefix.path().join(mode);
        let output = Command::new("cc")
            .arg("-std=c11")
            .arg("-I")
            .arg(root.join("../beskid_abi/include"))
            .arg(format!(
                "-DFIBER_FIXTURE_SYMBOL=\"{}{}\"",
                if cfg!(target_os = "macos") { "_" } else { "" },
                native_symbol
            ))
            .arg(root.join("tests/fixtures/fiber_value_transfer.c"))
            .arg(&object_path)
            .arg(library)
            .args(["-lpthread", "-lm", "-o"])
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        let output = Command::new(executable)
            .env(
                if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
                kit.shared_library.parent().unwrap(),
            )
            .output()
            .unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
