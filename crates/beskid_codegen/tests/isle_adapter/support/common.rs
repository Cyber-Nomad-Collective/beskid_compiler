use super::lookup::find_function_definition;
use super::prelude::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, AtomicUsize, BeskidDatabase,
    CANONICAL_BOOTSTRAP_SOURCE_PATH, CodegenInput, EffectiveCompilationRoots, ModuleIndex, Ordering, ProgramAssembly,
    ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId, TargetMetadata, build_typed_program,
    canonical_runtime_sources, isa, parse_program_with_source_name, settings,
};

#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
pub(in super::super) unsafe extern "C" fn test_system_allocate(size: usize, alignment: usize) -> *mut u8 {
    let Ok(layout) = std::alloc::Layout::from_size_align(size, alignment) else {
        return std::ptr::null_mut();
    };
    // The JIT regression reads the returned header and roots it immediately; intentionally keep
    // this test allocation alive until process exit because the canonical source owns no sweep.
    unsafe { std::alloc::alloc_zeroed(layout) }
}

#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
pub(in super::super) static TEST_CURRENT_TLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
pub(in super::super) unsafe extern "C" fn test_tls_get() -> *mut u8 {
    TEST_CURRENT_TLS.load(Ordering::SeqCst) as *mut u8
}

pub(in super::super) fn item_fixture(
    source: &str,
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let (input, isa, root) = item_fixture_with_root(source);
    let item = find_function_definition(input.database(), root).expect("function key");
    (input, isa, item)
}

pub(in super::super) fn item_fixture_with_root(
    source: &str,
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source).expect("parse source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(&*db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(21);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

pub(in super::super) fn function_signature(
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    result: cranelift_codegen::ir::Type,
    parameters: impl IntoIterator<Item = cranelift_codegen::ir::Type>,
) -> cranelift_codegen::ir::Signature {
    let mut signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    signature.params.extend(parameters.into_iter().map(cranelift_codegen::ir::AbiParam::new));
    signature.returns.push(cranelift_codegen::ir::AbiParam::new(result));
    signature
}

pub(in super::super) fn canonical_runtime_test_assembly(
    _db: &mut BeskidDatabase,
    directory: &std::path::Path,
) -> (Arc<ProgramAssembly>, std::path::PathBuf) {
    let all_sources = canonical_runtime_sources();
    let mut source_units = Vec::with_capacity(all_sources.len());
    for canonical in &all_sources {
        let sp = directory.join(&canonical.logical_path);
        if let Some(parent) = sp.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&sp, &canonical.source).expect("write canonical runtime source");
        let program = parse_program_with_source_name(sp.to_str().unwrap(), &canonical.source)
            .expect("parse canonical runtime source");
        source_units.push(SourceUnit {
            logical_name: canonical.logical_path.clone(),
            path: sp,
            source: canonical.source.clone(),
            program,
        });
    }
    let source_path = directory.join(CANONICAL_BOOTSTRAP_SOURCE_PATH);
    (
        Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory.to_path_buf() },
                dependencies: Vec::new(),
            },
            Arc::new(source_units),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            SyntaxGenerationId(0),
        )),
        source_path,
    )
}
