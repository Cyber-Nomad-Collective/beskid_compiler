#![cfg(unix)]

use beskid_abi::{
    abi_v5::AbiManifestV5,
    runtime_kit::BuildProfile,
    runtime_source::{
        CANONICAL_FOUNDATION_TIME_SOURCE_PATH, canonical_corelib_service_capability,
        canonical_corelib_service_source_path, canonical_corelib_service_sources,
    },
};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
    syntax::{FunctionDefinition, Visibility},
    syntax_query::{NodeKind, SyntaxIndex},
};
use beskid_codegen::{
    CodegenArtifact, CodegenInput, SyntaxModuleItem, lower_syntax_assembly_entrypoint, lower_syntax_program,
    syntax_item_signature,
};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ItemSignature, SemanticTypeId, SourceUnitId, SyntaxGenerationId,
    build_typed_program_with_corelib_services, direct_callees, item_abi_signature, item_name,
    project_session_for_syntax_assembly, reachable_items,
};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{collections::HashSet, path::Path, process::Command, sync::Arc};

#[test]
fn source_timer_sleep_deadline_validation_uses_production_helpers() {
    // Preflight only: native observations of the validation outcomes are a separate slice.
    let mut db = BeskidDatabase::default();
    let artifact = timer_validation_artifact(&mut db).expect("production timer helper selection and lowering");
    let db = artifact.input.database();
    assert_eq!(artifact.selected.len(), 3);
    for function in &artifact.selected {
        assert!(artifact.items.iter().any(|item| item.key == function.item.key));
        assert!(artifact.artifact.functions.iter().any(|emitted| emitted.name == function.item.symbol));
        assert_eq!(function.signature.result, SemanticTypeId::POINTER);
    }
    assert!(artifact.sleep_reachable.contains(&artifact.selected[1].item.key));
    assert!(artifact.sleep_reachable.contains(&artifact.selected[2].item.key));
    assert!(
        require_timer_validation_edges(
            db,
            artifact.sleep,
            &[artifact.selected[0].item.key, artifact.selected[2].item.key],
        )
        .is_err(),
        "a genuine but unrelated FromNanoseconds item cannot replace the deadline validation edge"
    );

    let time = artifact
        .input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| &unit.path == artifact.sleep.unit.path(db))
        .unwrap();
    require_canonical_timer_source(&time.path, &time.source).unwrap();
    assert!(require_canonical_timer_source(&time.path, time.source.trim_end()).is_err());
    let copied = tempfile::tempdir().unwrap();
    let copied_path = copied.path().join("Time.bd");
    std::fs::copy(&time.path, &copied_path).unwrap();
    assert!(require_canonical_timer_source(&copied_path, &time.source).is_err());
}

struct TimerValidationFunction {
    item: SyntaxModuleItem,
    signature: ItemSignature,
}

struct TimerValidationArtifact<'db> {
    input: CodegenInput<'db>,
    artifact: CodegenArtifact,
    sleep: AstNodeKey,
    sleep_reachable: Arc<[AstNodeKey]>,
    selected: Vec<TimerValidationFunction>,
    items: Vec<SyntaxModuleItem>,
}

fn require_canonical_timer_source(path: &Path, source: &str) -> anyhow::Result<()> {
    let expected_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_TIME_SOURCE_PATH)
        .ok_or_else(|| anyhow::anyhow!("canonical Core.Time path unavailable"))?;
    let expected = canonical_corelib_service_sources()
        .into_iter()
        .find(|unit| unit.logical_path == CANONICAL_FOUNDATION_TIME_SOURCE_PATH)
        .ok_or_else(|| anyhow::anyhow!("embedded Core.Time source unavailable"))?;
    anyhow::ensure!(path == expected_path && source == expected.source, "Core.Time source authority mismatch");
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(metadata.file_type().is_file(), "Core.Time must be a canonical regular file");
    Ok(())
}

fn require_timer_validation_edges(
    db: &dyn beskid_queries::Db,
    sleep: AstNodeKey,
    helpers: &[AstNodeKey; 2],
) -> anyhow::Result<Arc<[AstNodeKey]>> {
    let direct = direct_callees(db, sleep)?.ok_or_else(|| anyhow::anyhow!("Sleep direct callees unavailable"))?;
    anyhow::ensure!(
        direct.iter().copied().collect::<HashSet<_>>() == HashSet::from(*helpers),
        "Sleep must directly call exactly its canonical validation helpers"
    );
    let root = AstNodeKey { node: AstNodeId(0), ..sleep };
    let reachable =
        reachable_items(db, root, sleep)?.ok_or_else(|| anyhow::anyhow!("Sleep reachable closure incomplete"))?;
    anyhow::ensure!(helpers.iter().all(|helper| reachable.contains(helper)), "Sleep validation closure mismatch");
    Ok(reachable)
}

fn timer_validation_artifact(db: &mut BeskidDatabase) -> anyhow::Result<TimerValidationArtifact<'_>> {
    let assembly = source_transfer_assembly("timer");
    let time = assembly
        .units
        .iter()
        .filter(|unit| unit.logical_name == CANONICAL_FOUNDATION_TIME_SOURCE_PATH)
        .collect::<Vec<_>>();
    anyhow::ensure!(time.len() == 1, "one canonical Core.Time unit is required");
    let time = time[0];
    require_canonical_timer_source(&time.path, &time.source)?;
    let generation = assembly.generation;
    let index = SyntaxIndex::from_program(&time.program, generation);
    let unit = SourceUnitId::new(db, time.path.clone());
    let select = |name: &str, visibility: Visibility| -> anyhow::Result<AstNodeKey> {
        let candidates = index
            .ids_of_kind(NodeKind::FunctionDefinition)
            .filter(|node| {
                index
                    .node_at(&time.program, *node)
                    .and_then(|node| node.of::<FunctionDefinition>())
                    .is_some_and(|function| function.name.node.name == name && function.visibility.node == visibility)
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(candidates.len() == 1, "expected one {visibility:?} Core.Time.{name}");
        Ok(AstNodeKey { unit, generation, node: candidates[0] })
    };
    let sleep = select("Sleep", Visibility::Public)?;
    let selected_keys = [
        select("FromNanoseconds", Visibility::Public)?,
        select("SleepDeadlineFromSample", Visibility::Private)?,
        select("SleepResultFromStatus", Visibility::Private)?,
    ];
    let target = beskid_engine::host_runtime_target()?;
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let project = project_session_for_syntax_assembly(db, &assembly, "timer-validation", "canonical-helpers")?;
    let capability = canonical_corelib_service_capability(&manifest)
        .map_err(|error| anyhow::anyhow!("Corelib service capability: {error:?}"))?;
    let typed = build_typed_program_with_corelib_services(db, project, generation, assembly.clone(), capability)?;
    let roots = assembly
        .units
        .iter()
        .map(|source| AstNodeKey { unit: SourceUnitId::new(db, source.path.clone()), generation, node: AstNodeId(0) })
        .collect::<Vec<_>>();
    let input = CodegenInput::new(db, typed, roots.into(), target, manifest)?;
    let sleep_reachable = require_timer_validation_edges(db, sleep, &[selected_keys[1], selected_keys[2]])?;
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for entry in selected_keys {
        let root = AstNodeKey { node: AstNodeId(0), ..entry };
        let reachable = reachable_items(db, root, entry)?
            .ok_or_else(|| anyhow::anyhow!("timer helper reachable closure incomplete"))?;
        for key in reachable.iter().copied().filter(|key| seen.insert(*key)) {
            let name = item_name(db, key)?.ok_or_else(|| anyhow::anyhow!("unnamed timer closure item"))?;
            let source = assembly.units.iter().find(|source| &source.path == key.unit.path(db)).unwrap();
            let logical = source
                .logical_name
                .chars()
                .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
                .collect::<String>();
            items.push(SyntaxModuleItem { key, symbol: format!("{name}#syntax_{logical}_{}", key.node.0) });
        }
    }
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder()?;
    let isa = cranelift_native::builder()
        .map_err(anyhow::Error::msg)?
        .finish(cranelift_codegen::settings::Flags::new(settings))?;
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)?;
    let expected_parameters: &[&[SemanticTypeId]] =
        &[&[SemanticTypeId::I64], &[SemanticTypeId::POINTER, SemanticTypeId::I64], &[SemanticTypeId::WORD]];
    let mut selected = Vec::new();
    for (key, parameters) in selected_keys.into_iter().zip(expected_parameters) {
        let item = items
            .iter()
            .find(|item| item.key == key)
            .ok_or_else(|| anyhow::anyhow!("selected helper missing from its reachable closure"))?
            .clone();
        let signature = item_abi_signature(db, key)?.ok_or_else(|| anyhow::anyhow!("helper signature unavailable"))?;
        anyhow::ensure!(
            signature.parameters.as_ref() == *parameters && signature.result == SemanticTypeId::POINTER,
            "unexpected production helper ABI for {}",
            item.symbol
        );
        let emitted = artifact.functions.iter().filter(|function| function.name == item.symbol).collect::<Vec<_>>();
        anyhow::ensure!(emitted.len() == 1, "expected one emitted helper {}", item.symbol);
        let expected = syntax_item_signature(&input, isa.as_ref(), key)
            .map_err(|error| anyhow::anyhow!("helper CLIF signature: {error:?}"))?;
        anyhow::ensure!(
            emitted[0].function.signature == expected,
            "emitted helper signature disagrees with semantic facts for {}",
            item.symbol
        );
        selected.push(TimerValidationFunction { item, signature });
    }
    Ok(TimerValidationArtifact { input, artifact, sleep, sleep_reachable, selected, items })
}

#[test]
fn source_timer_sleep_has_typed_outcomes_and_owned_registration() {
    source_transfer_fixture("timer", "RunTimerFixture", 126);
}

#[test]
fn source_fiber_values_survive_collection_in_jit_aot_and_native_kit() {
    source_transfer_fixture("fiber", "RunFiberFixture", 126);
}

#[test]
fn source_fiber_error_direct_join_matches_survive_collection_in_jit_aot_and_native_kit() {
    source_transfer_fixture("fiber_error", "RunFiberErrorFixture", 126);
}

#[test]
fn source_channel_values_survive_collection_in_jit_aot_and_native_kit() {
    source_transfer_fixture("channel", "RunChannelFixture", 126);
}

#[test]
fn source_channel_disposable_resource_ownership_survives_close_cancellation_and_receipt_cleanup() {
    source_transfer_fixture("channel_receipt", "RunChannelReceiptFixture", 126);
}

#[test]
fn source_external_cancellation_is_sticky_across_new_waits_but_not_fiber_reuse() {
    source_transfer_fixture("external_cancel", "RunExternalCancellationFixture", 126);
}

fn source_transfer_assembly(kind: &str) -> Arc<ProgramAssembly> {
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
        (&foundation, "Core/Disposable.bd"),
        (&foundation, "Core/Results/Results.bd"),
    ] {
        paths.push((base.join(relative), relative));
    }
    if kind.starts_with("timer") {
        for relative in [
            "Core/Time/Time.bd",
            "Core/Time/Duration.bd",
            "Core/Time/Instant.bd",
            "Core/Time/Date.bd",
            "Core/Time/DateTime.bd",
            "Core/Time/TimeOfDay.bd",
            "Core/Time/TimeError.bd",
            "Core/Time/TimerError.bd",
        ] {
            paths.push((foundation.join(relative), relative));
        }
    }
    let units = paths
        .into_iter()
        .map(|(path, logical_name)| {
            let source = std::fs::read_to_string(&path).unwrap();
            let program = parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap();
            SourceUnit { path, logical_name: logical_name.into(), source, program }
        })
        .collect();
    Arc::new(ProgramAssembly::new(
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
    ))
}

fn source_transfer_fixture(kind: &str, entry: &str, expected: i64) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assembly = source_transfer_assembly(kind);
    let target = beskid_engine::host_runtime_target().unwrap();
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder().unwrap();
    let isa = cranelift_native::builder().unwrap().finish(cranelift_codegen::settings::Flags::new(settings)).unwrap();
    let mut db = BeskidDatabase::default();
    let lowered = lower_syntax_assembly_entrypoint(&mut db, assembly, entry, target.clone(), isa.as_ref())
        .expect("typed Fiber source lowering");
    eprintln!("Fiber source lowering complete");
    let prefix = tempfile::tempdir().unwrap();
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).unwrap();
    eprintln!(
        "{kind} native kit: target={} source={} static={} shared={}",
        kit.metadata.target.triple.as_str(),
        kit.metadata.source_hash,
        kit.metadata.artifacts.static_library.sha256,
        kit.metadata.artifacts.shared_library.sha256,
    );
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
        eprintln!("{kind} {mode} execution complete");
    }
}
