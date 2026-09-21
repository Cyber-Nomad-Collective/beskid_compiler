#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

#[path = "support/native_fixture.rs"]
mod native_fixture;

use anyhow::Context;
use beskid_abi::{
    abi_v5::{AbiManifestV5, TRAP_DIAGNOSTIC_PREFIX},
    runtime_kit::BuildProfile,
    runtime_source::{
        CANONICAL_FOUNDATION_TIME_SOURCE_PATH, canonical_corelib_service_capability,
        canonical_corelib_service_source_path, canonical_corelib_service_sources,
    },
};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name,
    syntax::{EnumDefinition, FunctionDefinition, Visibility},
    syntax_query::{NodeKind, SyntaxIndex},
};
use beskid_codegen::{
    CodegenArtifact, CodegenInput, SyntaxModuleItem, lower_syntax_assembly_entrypoint, lower_syntax_program,
    syntax_item_signature,
};
use beskid_queries::{
    AggregateFieldShape, AstNodeId, AstNodeKey, BeskidDatabase, EnumLayoutFact, ItemSignature, SemanticTypeId,
    SourceUnitId, SyntaxGenerationId, build_typed_program_with_corelib_services, direct_callees, enum_layout,
    item_abi_signature, item_name, project_session_for_syntax_assembly, reachable_items,
};
#[cfg(windows)]
use beskid_tests_support::native_harness::place_shared_runtime;
use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded, shared_library_name};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};
use std::{collections::HashSet, path::Path, process::Command, sync::Arc, time::Duration};

const ROUTE_LIMIT: Duration = Duration::from_secs(180);
const TIMER_FATAL_TEST: &str = "source_timer_sleep_invalid_runtime_status_fails_closed";
const TIMER_FATAL_CASE: &str = "BESKID_TEST_TIMER_FATAL_CASE";
const TIMER_FATAL_ROUTE: &str = "BESKID_TEST_TIMER_FATAL_ROUTE";
const TIMER_FATAL_DIAGNOSTIC: &str = "Core.Time.Sleep observed an invalid runtime status";

#[test]
fn source_timer_sleep_invalid_runtime_status_fails_closed() {
    let mut db = BeskidDatabase::default();
    let artifact = timer_validation_artifact(&mut db).expect("production timer helper selection and lowering");
    if let Some(case) = std::env::var_os(TIMER_FATAL_CASE) {
        assert_eq!(std::env::var(TIMER_FATAL_ROUTE).unwrap(), "engine");
        assert!(
            timer_fatal_cases().iter().any(|expected| case.as_os_str() == std::ffi::OsStr::new(expected)),
            "unknown fatal child case"
        );
        timer_validation_observations(&artifact, TimerValidationMode::EngineFatalChild).unwrap();
        panic!("engine fatal child returned without terminating");
    }
    for (route, executed) in timer_validation_observations(&artifact, TimerValidationMode::FatalParent).unwrap() {
        assert_eq!(executed, 4, "{route}: all four isolated invalid-status cases must execute");
    }
}

fn timer_fatal_cases() -> [String; 4] {
    // Metadata checks require the selected target's word width to equal usize.
    ["1".into(), "2".into(), "5".into(), usize::MAX.to_string()]
}

fn run_timer_fatal_child(command: &mut Command, route: &str, case: &str) -> anyhow::Result<()> {
    let output = run_bounded(
        &format!("{route}/{case}: fatal child"),
        command.env(TIMER_FATAL_CASE, case).env(TIMER_FATAL_ROUTE, route),
        ROUTE_LIMIT,
    );
    require_timer_fatal_output(route, case, &output)
}

fn require_timer_fatal_output(route: &str, case: &str, output: &std::process::Output) -> anyhow::Result<()> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let marker = format!("timer fatal route={route} status={case}");
    anyhow::ensure!(stderr.lines().any(|line| line == marker), "{route}/{case}: missing invocation marker: {stderr}");
    anyhow::ensure!(!output.status.success(), "{route}/{case}: invalid status returned successfully");
    let diagnostic = format!("{TRAP_DIAGNOSTIC_PREFIX}: {TIMER_FATAL_DIAGNOSTIC}");
    anyhow::ensure!(
        stderr.lines().any(|line| line == diagnostic),
        "{route}/{case}: unrelated failure {:?}, missing invariant diagnostic: {stderr}",
        output.status
    );
    for line in stderr
        .lines()
        .filter(|line| line.starts_with("timer validation kit:") || *line == marker || *line == diagnostic)
    {
        eprintln!("{route}/{case}: {line}");
    }
    eprintln!(
        "timer fatal passed route={route} status={case} exit={} diagnostic={TIMER_FATAL_DIAGNOSTIC}",
        output.status
    );
    Ok(())
}

#[test]
fn source_timer_sleep_deadline_validation_uses_production_helpers() {
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
        .find(|unit| SourceUnitId::new(db, unit.path.clone()) == artifact.sleep.unit)
        .unwrap();
    require_canonical_timer_source(&time.path, &time.source).unwrap();
    assert!(require_canonical_timer_source(&time.path, time.source.trim_end()).is_err());
    let copied = tempfile::tempdir().unwrap();
    let copied_path = copied.path().join("Time.bd");
    std::fs::copy(&time.path, &copied_path).unwrap();
    assert!(require_canonical_timer_source(&copied_path, &time.source).is_err());
    for (route, executed) in timer_validation_observations(&artifact, TimerValidationMode::Nonfatal).unwrap() {
        assert_eq!(executed, 9, "{route}: all six deadline and three status cases must execute");
    }
}

// These repr(C) records describe the callback transport only. Object offsets,
// discriminants, and unit's absence of storage come from semantic layout facts.
#[repr(C)]
#[derive(Debug)]
struct TimerResultLayout {
    size: usize,
    tag_offset: usize,
    ok_tag: usize,
    error_tag: usize,
    ok_offset: usize,
    error_offset: usize,
}

#[repr(C)]
struct TimerValidationMetadata {
    pointer_bytes: usize,
    word_bytes: usize,
    i64_bytes: usize,
    tag_bytes: usize,
    deadline: TimerResultLayout,
    status: TimerResultLayout,
    error_size: usize,
    error_tag_offset: usize,
    unavailable_tag: usize,
    overflow_tag: usize,
    cancelled_tag: usize,
    from_nanoseconds: Option<unsafe extern "C" fn(i64) -> *mut u8>,
    deadline_from_sample: Option<unsafe extern "C" fn(*mut u8, i64) -> *mut u8>,
    result_from_status: Option<unsafe extern "C" fn(usize) -> *mut u8>,
}

fn timer_validation_metadata(artifact: &TimerValidationArtifact<'_>) -> anyhow::Result<TimerValidationMetadata> {
    let input = &artifact.input;
    let db = input.database();
    let width = input.target().pointer_width;
    let bytes = |ty: SemanticTypeId| -> anyhow::Result<usize> {
        Ok(ty.scalar_abi_layout(width).context("timer scalar ABI layout unavailable")?.size.try_into()?)
    };
    anyhow::ensure!(bytes(SemanticTypeId::POINTER)? == size_of::<*mut u8>());
    anyhow::ensure!(bytes(SemanticTypeId::WORD)? == size_of::<usize>());
    anyhow::ensure!(bytes(SemanticTypeId::I64)? == size_of::<i64>());
    anyhow::ensure!(bytes(SemanticTypeId::I32)? == size_of::<u32>());
    let settings = beskid_codegen::cranelift_host::production_isa_settings_builder()?;
    let isa = cranelift_native::builder()
        .map_err(anyhow::Error::msg)?
        .finish(cranelift_codegen::settings::Flags::new(settings))?;
    use cranelift_codegen::ir::{AbiParam, Signature, types};
    // Independently check the C transport signatures, including calling convention,
    // extensions and hidden parameters, against both queries and emitted code.
    for (selected, parameters) in
        artifact.selected.iter().zip([vec![types::I64], vec![isa.pointer_type(), types::I64], vec![isa.pointer_type()]])
    {
        let mut c_signature = Signature::new(isa.default_call_conv());
        c_signature.params.extend(parameters.into_iter().map(AbiParam::new));
        c_signature.returns.push(AbiParam::new(isa.pointer_type()));
        let query = syntax_item_signature(input, isa.as_ref(), selected.item.key)
            .map_err(|error| anyhow::anyhow!("timer signature query: {error:?}"))?;
        anyhow::ensure!(query == c_signature, "{} cannot use the C callback ABI", selected.item.symbol);
        let emitted = artifact
            .artifact
            .functions
            .iter()
            .find(|function| function.name == selected.item.symbol)
            .context("selected timer emission missing")?;
        anyhow::ensure!(emitted.function.signature == c_signature, "{} emitted C ABI mismatch", selected.item.symbol);
    }
    let header = input
        .abi_manifest()
        .layouts
        .iter()
        .find(|layout| layout.name == "BeskidObjectHeader")
        .context("managed object header unavailable")?;
    let physical = |fact: &EnumLayoutFact| {
        fact.scalar_payload_object_layout(width, header.size, header.alignment)
            .context("timer physical enum layout unavailable")
    };
    let variant = |fact: &EnumLayoutFact, name: &str| {
        fact.variants
            .iter()
            .position(|variant| variant.name.as_ref() == name)
            .with_context(|| format!("timer enum variant {name} unavailable"))
    };
    let result_layout = |key: AstNodeKey, ok_type| -> anyhow::Result<(TimerResultLayout, AstNodeKey)> {
        let unit = input
            .typed_program()
            .assembly
            .units
            .iter()
            .find(|unit| SourceUnitId::new(db, unit.path.clone()) == key.unit)
            .context("timer helper source unavailable")?;
        let index = SyntaxIndex::from_program(&unit.program, key.generation);
        let mut pending = vec![key.node];
        let mut layouts = Vec::new();
        while let Some(node) = pending.pop() {
            pending.extend(index.children(node).unwrap_or_default());
            if index.kind(node) == Some(NodeKind::EnumConstructorExpression) {
                let fact =
                    enum_layout(db, AstNodeKey { node, ..key })?.context("timer constructor layout unavailable")?;
                if fact.variants.iter().any(|variant| variant.name.as_ref() == "Ok") {
                    layouts.push(fact);
                }
            }
        }
        let fact = layouts.first().context("no specialized Result constructors in timer helper")?;
        anyhow::ensure!(layouts.iter().all(|layout| layout == fact), "inconsistent specialized Result layouts");
        anyhow::ensure!(fact.variants.len() == 2, "Result must have exactly Ok and Error");
        let ok = variant(fact, "Ok")?;
        let error = variant(fact, "Error")?;
        anyhow::ensure!(fact.variants[ok].fields.len() == 1 && fact.variants[error].fields.len() == 1);
        anyhow::ensure!(
            fact.variants[ok].fields[0].1 == AggregateFieldShape::Scalar(ok_type),
            "Result specialization mismatch"
        );
        let AggregateFieldShape::Nominal(error_key) = fact.variants[error].fields[0].1 else {
            anyhow::bail!("Result error must retain its nominal TimerError identity");
        };
        let layout = physical(fact)?;
        let ok_offset = match layout.variants[ok].payload_fields[0] {
            None if ok_type == SemanticTypeId::UNIT => usize::MAX,
            Some((ty, offset)) if ty == ok_type => offset.try_into()?,
            _ => anyhow::bail!("Result Ok storage mismatch"),
        };
        let (error_type, error_offset) =
            layout.variants[error].payload_fields[0].context("missing TimerError storage")?;
        anyhow::ensure!(
            error_type == SemanticTypeId::POINTER && layout.pointer_map_offsets.contains(&error_offset),
            "Result must trace its TimerError payload"
        );
        Ok((
            TimerResultLayout {
                size: layout.object_size.try_into()?,
                tag_offset: layout.tag_offset.try_into()?,
                ok_tag: ok,
                error_tag: error,
                ok_offset,
                error_offset: error_offset.try_into()?,
            },
            error_key,
        ))
    };
    let (deadline, deadline_error) = result_layout(artifact.selected[1].item.key, SemanticTypeId::I64)?;
    let (status, status_error) = result_layout(artifact.selected[2].item.key, SemanticTypeId::UNIT)?;
    anyhow::ensure!(deadline_error == status_error, "timer helpers must return the same nominal error");
    let error_unit = input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| SourceUnitId::new(db, unit.path.clone()) == deadline_error.unit)
        .context("TimerError source unavailable")?;
    let error_index = SyntaxIndex::from_program(&error_unit.program, deadline_error.generation);
    anyhow::ensure!(
        error_index
            .node_at(&error_unit.program, deadline_error.node)
            .and_then(|node| node.of::<EnumDefinition>())
            .is_some_and(|definition| definition.name.node.name == "TimerError"),
        "unexpected error nominal"
    );
    let error = enum_layout(db, deadline_error)?.context("TimerError layout unavailable")?;
    anyhow::ensure!(error.variants.iter().all(|variant| variant.fields.is_empty()), "TimerError must have no payloads");
    let error_layout = physical(&error)?;
    eprintln!("timer query layouts: deadline={deadline:?} status={status:?} error={error_layout:?}");
    Ok(TimerValidationMetadata {
        pointer_bytes: bytes(SemanticTypeId::POINTER)?,
        word_bytes: bytes(SemanticTypeId::WORD)?,
        i64_bytes: bytes(SemanticTypeId::I64)?,
        tag_bytes: bytes(SemanticTypeId::I32)?,
        deadline,
        status,
        error_size: error_layout.object_size.try_into()?,
        error_tag_offset: error_layout.tag_offset.try_into()?,
        unavailable_tag: variant(&error, "Unavailable")?,
        overflow_tag: variant(&error, "DeadlineOverflow")?,
        cancelled_tag: variant(&error, "Cancelled")?,
        from_nanoseconds: None,
        deadline_from_sample: None,
        result_from_status: None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TimerValidationMode {
    Nonfatal,
    FatalParent,
    EngineFatalChild,
}

fn timer_validation_observations(
    artifact: &TimerValidationArtifact<'_>,
    validation_mode: TimerValidationMode,
) -> anyhow::Result<[(&'static str, usize); 3]> {
    let mut metadata = timer_validation_metadata(artifact)?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The parent owns fatal-child build directories because a process trap
    // cannot run the child's TempDir destructor.
    let prefix = if validation_mode == TimerValidationMode::EngineFatalChild {
        tempfile::tempdir_in(
            std::env::var_os("BESKID_TEST_TIMER_FATAL_BUILD_ROOT").context("missing child build root")?,
        )?
    } else {
        tempfile::tempdir()?
    };
    let kit = build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)?;
    eprintln!(
        "timer validation kit: target={} source={} static={} shared={}",
        kit.metadata.target.triple.as_str(),
        kit.metadata.source_hash,
        kit.metadata.artifacts.static_library.sha256,
        kit.metadata.artifacts.shared_library.sha256
    );
    #[cfg(unix)]
    let shared_link_library = &kit.shared_library;
    #[cfg(windows)]
    let shared_link_library = {
        let library = kit.shared_import_library.as_ref().expect("Windows ABI-v5 kit requires a COFF import library");
        eprintln!(
            "timer validation kit: import={} path={}",
            kit.metadata.artifacts.shared_import_library.as_ref().expect("Windows import metadata").sha256,
            library.display()
        );
        library
    };
    let driver = root.join("tests/fixtures/timer_validation.c");
    let compiler = || {
        let mut cc = native_c_compiler();
        cc.args(["-Wall", "-Wextra", "-Werror", "-I"]).arg(root.join("../beskid_abi/include"));
        cc
    };
    // The callback shares the exact DLL directory loaded by Engine below.
    let fixture_library = kit.shared_library.parent().unwrap().join(shared_library_name("timer-validation"));
    let mut cc = compiler();
    #[cfg(target_os = "macos")]
    cc.arg("-dynamiclib");
    #[cfg(target_os = "linux")]
    cc.args(["-shared", "-fPIC"]);
    #[cfg(windows)]
    cc.arg("-shared");
    cc.arg(&driver).arg(shared_link_library);
    #[cfg(unix)]
    cc.args(["-lpthread", "-lm"]);
    cc.arg("-o").arg(&fixture_library).current_dir(fixture_library.parent().unwrap());
    let output = run_bounded("engine timer driver build", &mut cc, ROUTE_LIMIT);
    anyhow::ensure!(output.status.success(), "engine driver build: {}", String::from_utf8_lossy(&output.stderr));
    let mut counts = [("engine", 0), ("static", 0), ("shared", 0)];
    if validation_mode == TimerValidationMode::FatalParent {
        for case in timer_fatal_cases() {
            let mut command = Command::new(std::env::current_exe()?);
            command.args([TIMER_FATAL_TEST, "--exact", "--nocapture", "--test-threads=1"]);
            command.env("BESKID_TEST_TIMER_FATAL_BUILD_ROOT", prefix.path());
            run_timer_fatal_child(&mut command, "engine", &case)?;
            counts[0].1 += 1;
        }
    } else {
        let mut engine = beskid_engine::Engine::with_runtime_kit(
            prefix.path(),
            artifact.input.target().clone(),
            BuildProfile::Debug,
        )?;
        engine.compile_artifact(&artifact.artifact).context("engine timer compilation")?;
        // SAFETY: the C fixture defines this exact repr(C) transport and entry ABI.
        // Every generated callback was checked against queries and emitted CLIF above;
        // engine, artifact, shared runtime, and fixture remain alive for the call.
        unsafe {
            metadata.from_nanoseconds = Some(std::mem::transmute::<*const u8, unsafe extern "C" fn(i64) -> *mut u8>(
                engine.entrypoint_ptr(&artifact.selected[0].item.symbol)?,
            ));
            metadata.deadline_from_sample =
                Some(std::mem::transmute::<*const u8, unsafe extern "C" fn(*mut u8, i64) -> *mut u8>(
                    engine.entrypoint_ptr(&artifact.selected[1].item.symbol)?,
                ));
            metadata.result_from_status =
                Some(std::mem::transmute::<*const u8, unsafe extern "C" fn(usize) -> *mut u8>(
                    engine.entrypoint_ptr(&artifact.selected[2].item.symbol)?,
                ));
            let fixture =
                native_fixture::NativeFixture::<unsafe extern "C" fn(*const TimerValidationMetadata) -> usize>::load(
                    &fixture_library,
                    c"RunTimerValidation",
                );
            counts[0].1 = (fixture.entry)(&metadata);
        }
    }
    if validation_mode == TimerValidationMode::EngineFatalChild {
        anyhow::bail!("engine fatal invocation unexpectedly returned");
    }
    let object_path = prefix.path().join(if cfg!(windows) { "timer.obj" } else { "timer.o" });
    let symbols = artifact
        .selected
        .iter()
        .map(|function| beskid_codegen::object_link_symbol(&function.item.symbol, &artifact.artifact.exports))
        .collect::<Vec<_>>();
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug)
        .context("static/shared timer object module")?;
    object
        .compile_artifact_with_exports(&artifact.artifact, &symbols.iter().cloned().collect(), None)
        .context("static/shared timer object compilation")?;
    object.finalize_to_path(&object_path).context("static/shared timer object emission")?;
    let c_layout = |layout: &TimerResultLayout| {
        format!(
            "{{{},{},{},{},{}ULL,{}}}",
            layout.size, layout.tag_offset, layout.ok_tag, layout.error_tag, layout.ok_offset, layout.error_offset
        )
    };
    let c_metadata = format!(
        "{{{},{},{},{},{},{},{},{},{},{},{},NULL,NULL,NULL}}",
        metadata.pointer_bytes,
        metadata.word_bytes,
        metadata.i64_bytes,
        metadata.tag_bytes,
        c_layout(&metadata.deadline),
        c_layout(&metadata.status),
        metadata.error_size,
        metadata.error_tag_offset,
        metadata.unavailable_tag,
        metadata.overflow_tag,
        metadata.cancelled_tag
    );
    for (slot, library) in [(1, &kit.static_library), (2, shared_link_library)] {
        let mode = counts[slot].0;
        let directory = prefix.path().join(mode);
        std::fs::create_dir(&directory)?;
        let executable = directory.join(executable_name("timer-validation"));
        let mut cc = compiler();
        cc.arg("-DTIMER_VALIDATION_STANDALONE").arg(format!("-DTIMER_VALIDATION_METADATA={c_metadata}"));
        for (define, symbol) in
            ["TIMER_FROM_NANOSECONDS_SYMBOL", "TIMER_DEADLINE_SYMBOL", "TIMER_STATUS_SYMBOL"].iter().zip(&symbols)
        {
            cc.arg(format!("-D{define}=\"{}{symbol}\"", if cfg!(target_os = "macos") { "_" } else { "" }));
        }
        cc.arg(&driver).arg(&object_path).arg(library);
        #[cfg(unix)]
        cc.args(["-lpthread", "-lm"]);
        cc.arg("-o").arg(&executable).current_dir(&directory);
        let output = run_bounded(&format!("{mode} timer driver build"), &mut cc, ROUTE_LIMIT);
        anyhow::ensure!(
            output.status.success(),
            "{mode} timer driver build: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        #[cfg(windows)]
        if mode == "shared" {
            place_shared_runtime(&directory, &kit.shared_library);
        }
        let mut command = Command::new(&executable);
        #[cfg(unix)]
        command.env(
            if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
            kit.shared_library.parent().unwrap(),
        );
        if validation_mode == TimerValidationMode::FatalParent {
            for case in timer_fatal_cases() {
                run_timer_fatal_child(&mut command, counts[slot].0, &case)?;
                counts[slot].1 += 1;
            }
            continue;
        }
        let output = run_bounded(&format!("{mode} timer driver"), &mut command, ROUTE_LIMIT);
        anyhow::ensure!(
            output.status.success(),
            "{mode} timer driver execution {:?}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        counts[slot].1 = String::from_utf8(output.stdout)?
            .trim()
            .parse()
            .with_context(|| format!("{mode} timer case count missing"))?;
        eprintln!("{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
    eprintln!("timer validation executed: {counts:?}");
    Ok(counts)
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
            let source =
                assembly.units.iter().find(|source| SourceUnitId::new(db, source.path.clone()) == key.unit).unwrap();
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
    // Match the lexical CARGO_MANIFEST_DIR-based paths owned by source authority;
    // Windows canonicalize() would introduce a different verbatim-path prefix.
    let compiler = root.ancestors().nth(2).expect("Engine crate must be nested under compiler/crates");
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
    #[cfg(unix)]
    let shared_link_library = &kit.shared_library;
    #[cfg(windows)]
    let shared_link_library = {
        let library = kit.shared_import_library.as_ref().expect("Windows ABI-v5 kit requires a COFF import library");
        eprintln!(
            "{kind} native kit: import={} path={}",
            kit.metadata.artifacts.shared_import_library.as_ref().expect("Windows import metadata").sha256,
            library.display()
        );
        library
    };
    {
        let mut engine = beskid_engine::Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).unwrap();
        engine.compile_artifact(&lowered.artifact).expect("JIT typed Fiber artifact");
        let entry = unsafe { engine.entrypoint_ptr(&lowered.symbol) }.unwrap();
        let run: extern "C" fn() -> i64 = unsafe { std::mem::transmute(entry) };
        eprintln!("Fiber JIT execution start");
        assert_eq!(run(), expected);
        eprintln!("{kind} engine execution complete result={expected}");
    }
    let object_path = prefix.path().join(if cfg!(windows) { "fiber.obj" } else { "fiber.o" });
    let native_symbol = beskid_codegen::object_link_symbol(&lowered.symbol, &lowered.artifact.exports);
    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).unwrap();
    object.compile_artifact_with_exports(&lowered.artifact, &HashSet::from([native_symbol.clone()]), None).unwrap();
    object.finalize_to_path(&object_path).unwrap();
    for (mode, library) in [("aot", &kit.static_library), ("native-kit", shared_link_library)] {
        let directory = prefix.path().join(mode);
        std::fs::create_dir(&directory).unwrap();
        let executable = directory.join(executable_name("fiber-value-transfer"));
        let mut cc = native_c_compiler();
        cc.arg("-I")
            .arg(root.join("../beskid_abi/include"))
            .arg(format!(
                "-DFIBER_FIXTURE_SYMBOL=\"{}{}\"",
                if cfg!(target_os = "macos") { "_" } else { "" },
                native_symbol
            ))
            .arg(root.join("tests/fixtures/fiber_value_transfer.c"))
            .arg(&object_path)
            .arg(library);
        #[cfg(unix)]
        cc.args(["-lpthread", "-lm"]);
        cc.arg("-o").arg(&executable).current_dir(&directory);
        let output = run_bounded(&format!("{kind} {mode} build"), &mut cc, ROUTE_LIMIT);
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        #[cfg(windows)]
        if mode == "native-kit" {
            place_shared_runtime(&directory, &kit.shared_library);
        }
        let mut command = Command::new(&executable);
        #[cfg(unix)]
        command.env(
            if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" },
            kit.shared_library.parent().unwrap(),
        );
        let output = run_bounded(&format!("{kind} {mode} execution"), &mut command, ROUTE_LIMIT);
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        eprintln!("{kind} {mode} execution complete result={expected}");
    }
}
