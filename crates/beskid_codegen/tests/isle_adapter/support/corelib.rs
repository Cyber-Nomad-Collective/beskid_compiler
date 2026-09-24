use super::lookup::find_function_definitions;
use super::prelude::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    CANONICAL_CORELIB_SYSCALL_SOURCE_PATH, CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxIndex, SyntaxModuleItem, TargetMetadata, build_typed_program_with_corelib_services,
    canonical_corelib_service_capability, canonical_corelib_service_source_path, canonical_corelib_syscall_sources,
    isa, item_name, lower_syntax_program, parse_program_with_source_name, settings,
};

pub(in super::super) fn canonical_corelib_syscall_fixture()
-> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    let mut db = Box::new(BeskidDatabase::default());
    let source_path = canonical_corelib_service_source_path(CANONICAL_CORELIB_SYSCALL_SOURCE_PATH)
        .expect("compiler-owned Core.Syscall source");
    let source_root = source_path.ancestors().nth(3).expect("foundation source root").to_path_buf();
    let source = std::fs::read_to_string(&source_path).expect("read compiler-owned Core.Syscall source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source)
        .expect("parse compiler-owned Core.Syscall source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-corelib".into(),
        "corelib-source".into(),
    );
    let generation = SyntaxGenerationId(92);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_CORELIB_SYSCALL_SOURCE_PATH.into(),
            origin_path: source_path.clone(),
            path: source_path.clone(),
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
    .expect("compiler-owned Core.Syscall source receives service authority");
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
        runtime_fixture: None,
        roots: EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.clone() },
            dependencies: vec![RootEntry {
                dependency_name: Some("corelib_foundation".into()),
                source_root: directory.join("obj/beskid/deps/src/foundation"),
            }],
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: source_path.display().to_string(),
            origin_path: source_path.clone(),
            path: source_path.clone(),
            source: source.source,
            program: program.clone(),
        }]),
        syntax_indexes: Arc::new(vec![SyntaxIndex::from_program(&program, generation)]),
        generation,
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
        trusted_corelib_service_paths: Arc::from([source_path.clone()]),
    };
    let syntax = Arc::new(assembly);
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
    // Every Core.Args fixture path ends in `Core/Args/Args.bd`; its package source root is where
    // logical module paths start, so `Core.Args` and the modules it imports resolve by name.
    let source_root = source_path.ancestors().nth(3).expect("Core.Args package source root").to_path_buf();
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
    let mut roots =
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() };
    let mut units = vec![SourceUnit {
        logical_name: CANONICAL_CORELIB_ARGS_SOURCE_PATH.into(),
        origin_path: source_path.clone(),
        path: source_path,
        source,
        program,
    }];
    include_imported_corelib_modules(&mut units, &mut roots);
    let syntax_indexes = units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect();
    let assembly = ProgramAssembly {
        runtime_fixture: None,
        roots,
        units: Arc::new(units),
        syntax_indexes: Arc::new(syntax_indexes),
        generation,
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
        trusted_corelib_service_paths,
    };
    let syntax = Arc::new(assembly);
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
    let mut roots =
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() };
    let mut units = vec![SourceUnit {
        logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
        origin_path: source_path.clone(),
        path: source_path,
        source: source.source,
        program,
    }];
    include_imported_corelib_modules(&mut units, &mut roots);
    let assembly = Arc::new(ProgramAssembly::new(
        roots,
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit {
            logical_name: source_relative_path.into(),
            origin_path: source_path.clone(),
            path: source_path,
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
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

/// Complete a partial Corelib fixture assembly with the modules its units import.
///
/// Every module a top-level `use` of `units` names and that no unit already provides is read from
/// the canonical Corelib package sources (Foundation, Network) and appended to `units`; the package
/// source root it lives under becomes a dependency root when it is not already a compilation root.
/// The legality gate rejects a `use` that names no assembled module (E1105), so a fixture that
/// lowers an item must carry the modules that item's unit imports. The appended units' own imports
/// stay outside the assembly: nothing is lowered from them, so the gate does not judge them.
pub(in super::super) fn include_imported_corelib_modules(
    units: &mut Vec<SourceUnit>,
    roots: &mut EffectiveCompilationRoots,
) {
    let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages");
    let package_roots = ["foundation", "network"]
        .map(|package| std::fs::canonicalize(packages.join(package).join("src")).expect("Corelib package source root"));
    let module_of = |unit: &SourceUnit, roots: &EffectiveCompilationRoots| {
        beskid_analysis::projects::infer_logical_module_path(unit, roots, false)
    };
    let mut present = units.iter().filter_map(|unit| module_of(unit, roots)).collect::<Vec<_>>();
    let imports = units
        .iter()
        .flat_map(|unit| unit.program.node.items.iter())
        .filter_map(|item| match &item.node {
            beskid_analysis::syntax::Node::UseDeclaration(declaration) => Some(
                declaration
                    .node
                    .path
                    .node
                    .segments
                    .iter()
                    .map(|segment| segment.node.name.node.name.clone())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .collect::<Vec<_>>();
    for module in imports {
        if present.contains(&module) {
            continue;
        }
        let relative = module.iter().collect::<std::path::PathBuf>();
        let last = module.last().expect("non-empty import path");
        let Some((package_root, path)) = package_roots.iter().find_map(|root| {
            [root.join(&relative).with_extension("bd"), root.join(&relative).join(format!("{last}.bd"))]
                .into_iter()
                .find(|candidate| candidate.is_file())
                .map(|candidate| (root, candidate))
        }) else {
            continue;
        };
        if roots.host.source_root != *package_root
            && !roots.dependencies.iter().any(|dependency| dependency.source_root == *package_root)
        {
            roots.dependencies.push(RootEntry {
                dependency_name: package_root
                    .parent()
                    .and_then(|package| package.file_name())
                    .map(|name| name.to_string_lossy().into_owned()),
                source_root: package_root.clone(),
            });
        }
        let source = std::fs::read_to_string(&path).expect("imported Corelib module source");
        let program = parse_program_with_source_name(path.to_str().unwrap(), &source).expect("parse imported module");
        let unit =
            SourceUnit { logical_name: path.display().to_string(), origin_path: path.clone(), path, source, program };
        assert_eq!(module_of(&unit, roots).as_ref(), Some(&module), "imported module resolves to its own path");
        present.push(module);
        units.push(unit);
    }
}
