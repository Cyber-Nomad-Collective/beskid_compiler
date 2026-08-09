use super::lookup::find_function_definitions;
use super::prelude::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxModuleItem, SyntaxProgramAssembly, TargetMetadata,
    build_canonical_corelib_syscall_typed_program, build_typed_program_with_corelib_services,
    canonical_corelib_service_capability, canonical_corelib_service_source_path,
    canonical_corelib_syscall_service_capability, canonical_corelib_syscall_sources, isa, item_name,
    lower_syntax_program, parse_program_with_source_name, settings,
};

pub(in super::super) fn canonical_corelib_syscall_fixture()
-> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("Corelib syscall project").keep();
    let source = canonical_corelib_syscall_sources().pop().expect("embedded Core.Syscall source");
    let source_path = directory.join("Syscall.bd");
    std::fs::write(&source_path, &source.source).expect("write embedded Core.Syscall source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse embedded Core.Syscall source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-corelib".into(),
        "corelib-source".into(),
    );
    let generation = SyntaxGenerationId(92);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            path: source_path,
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_corelib_syscall_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_syscall_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("exact embedded Core.Syscall source receives service authority");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest).expect("generation-safe Corelib input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

pub(in super::super) fn materialized_corelib_syscall_fixture()
-> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("materialized Corelib syscall project").keep();
    let source = canonical_corelib_syscall_sources().pop().expect("embedded Core.Syscall source");
    let source_path = directory.join("obj/beskid/deps/src/foundation/Core/Syscall/Syscall.bd");
    std::fs::create_dir_all(source_path.parent().expect("materialized syscall parent"))
        .expect("create materialized syscall parent");
    std::fs::write(&source_path, &source.source).expect("write materialized Core.Syscall source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse materialized Core.Syscall source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-corelib".into(),
        "materialized-corelib-source".into(),
    );
    let generation = SyntaxGenerationId(97);
    let assembly = ProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.clone() },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_foundation".into()),
                source_root: directory.join("obj/beskid/deps/src/foundation"),
            }],
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: source_path.display().to_string(),
            path: source_path.clone(),
            source: source.source,
            program,
        }]),
        hir_units: Arc::new(Vec::new()),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
        trusted_corelib_service_paths: Arc::from([source_path.clone()]),
    };
    let syntax = Arc::new(SyntaxProgramAssembly::from(&assembly));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        syntax,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("loader-proven materialized Core.Syscall receives service authority");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe materialized Corelib input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

pub(in super::super) fn core_args_fixture(
    source_path: std::path::PathBuf,
    source: String,
    trusted_corelib_service_paths: Arc<[std::path::PathBuf]>,
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let source_root = source_path.parent().expect("Core.Args source parent").to_path_buf();
    let program =
        parse_program_with_source_name(source_path.to_str().unwrap(), &source).expect("parse Core.Args source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-foundation".into(),
        "core-args-authority".into(),
    );
    let generation = SyntaxGenerationId(98);
    let assembly = ProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root },
            dependencies: Vec::new(),
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_ARGS_SOURCE_PATH.into(),
            path: source_path,
            source,
            program,
        }]),
        hir_units: Arc::new(Vec::new()),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
        trusted_corelib_service_paths,
    };
    let syntax = Arc::new(SyntaxProgramAssembly::from(&assembly));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        syntax,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("Core.Args typed program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input =
        CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest).expect("generation-safe Core.Args input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

pub(in super::super) fn named_function(input: &CodegenInput<'_>, root: AstNodeKey, name: &str) -> AstNodeKey {
    find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
        .unwrap_or_else(|| panic!("Core.Args source contains {name}"))
}

pub(in super::super) fn assert_args_module_cannot_emit_imports(
    input: &CodegenInput<'_>,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    root: AstNodeKey,
) {
    let error = lower_syntax_program(
        input,
        isa,
        &[SyntaxModuleItem { key: named_function(input, root, "ProgramName"), symbol: "ProgramName".into() }],
    )
    .expect_err("untrusted Core.Args source must fail module emission before any ABI import is emitted");
    assert!(
        error.to_string().contains("MissingRuleOrFact"),
        "untrusted Core.Args must fail closed through generated ISLE: {error}"
    );
}

pub(in super::super) fn canonical_foundation_assert_fixture()
-> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let source_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("compiler-owned Assert path");
    let source_root = source_path.ancestors().nth(2).expect("foundation source root").to_path_buf();
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse embedded Foundation Assert source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-foundation".into(),
        "compiler-owned-foundation".into(),
    );
    let generation = SyntaxGenerationId(94);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            path: source_path,
            source: source.source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("compiler-owned Assert source receives service authority");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe Foundation Assert input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

pub(in super::super) fn canonical_foundation_output_fixture()
-> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    canonical_foundation_service_fixture("Core/Output/Output.bd")
}

pub(in super::super) fn canonical_foundation_error_fixture()
-> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    canonical_foundation_service_fixture("Core/Error/Error.bd")
}

pub(in super::super) fn canonical_foundation_service_fixture(
    source_relative_path: &str,
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let source_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/foundation/src")
        .join(source_relative_path);
    let source_path = std::fs::canonicalize(&source_path).expect("canonical Foundation service path");
    let source = std::fs::read_to_string(&source_path).expect("embedded Foundation service source");
    let source_root = source_path.ancestors().nth(3).expect("foundation source root").to_path_buf();
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source)
        .expect("parse embedded Foundation Output source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-foundation".into(),
        "compiler-owned-foundation".into(),
    );
    let generation = SyntaxGenerationId(96);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit { logical_name: source_relative_path.into(), path: source_path, source, program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("compiler-owned Foundation service source parses without broadening authority");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe Foundation service input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}
