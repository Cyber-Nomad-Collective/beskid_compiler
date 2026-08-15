use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase,
    CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, CodegenInput, EffectiveCompilationRoots, ModuleIndex, NodeKind,
    ProjectSession, RootEntry, SourceUnit, SourceUnitId, SyntaxGenerationId, SyntaxIndex, SyntaxModuleItem,
    ProgramAssembly, TargetMetadata, build_typed_program, build_typed_program_with_corelib_services,
    call_lowering, canonical_corelib_service_capability, canonical_corelib_service_source_path,
    canonical_foundation_assert_fixture, enum_layout, find_corelib_service_call, find_definition_of_kind,
    find_function_definitions, isa, item_fixture_with_root, item_name, lower_syntax_program,
    parse_program_with_source_name, settings,
};

#[test]
fn unknown_qualified_payload_type_remains_unavailable_to_isle() {
    let (input, _isa, root) =
        item_fixture_with_root("enum Envelope { Item(Core.Missing value), } unit Main() { return; }");
    let definition = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::EnumDefinition)
        .expect("Envelope definition");

    assert!(
        enum_layout(input.database(), definition).is_err(),
        "a qualified payload without one exact assembled source module must remain unavailable"
    );
}

#[test]
fn user_copy_of_foundation_output_cannot_import_the_panic_service() {
    let mut db = BeskidDatabase::default();
    let workspace = tempfile::tempdir().expect("user lookalike workspace").keep();
    let source_path = workspace.join("Core/Output/Output.bd");
    std::fs::create_dir_all(source_path.parent().expect("user lookalike output parent"))
        .expect("create user lookalike output parent");
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corelib/packages/foundation/src/Core/Output/Output.bd"),
    )
    .expect("read canonical bytes to copy");
    std::fs::write(&source_path, &source).expect("write user lookalike source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source)
        .expect("parse user lookalike Output source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project =
        ProjectSession::new(&db, workspace.clone(), source_path.clone(), "user-output-copy".into(), "untrusted".into());
    let generation = SyntaxGenerationId(97);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: workspace },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit { logical_name: "Core/Output/Output.bd".into(), path: source_path, source, program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("user copy remains an ordinary syntax program");
    assert!(
        typed.corelib_service_capability.is_none(),
        "an untrusted physical path must not attach compiler Corelib authority"
    );
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let write = find_function_definitions(&db, root)
        .into_iter()
        .find(|key| item_name(&db, *key).ok().flatten().as_deref() == Some("Write"))
        .expect("user copy Write source item");
    assert!(
        find_corelib_service_call(&db, write, "__panic_str").is_none(),
        "identical user bytes at an untrusted physical path must not acquire the panic import"
    );
}
#[test]
fn canonical_foundation_assert_public_helpers_lower_through_syntax_isle() {
    let (input, isa, root) = canonical_foundation_assert_fixture();
    // Non-generic helpers and their direct callees. Contains stays out: it pulls Core.String.
    // Equal is exercised below with an explicit call-derived i64 specialization.
    let items = ["trigger_failure", "Fail", "fail_with_because", "True", "False"];
    let mut module_items = Vec::new();
    for name in items {
        let key = find_function_definitions(input.database(), root)
            .into_iter()
            .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("canonical Assert {name}"));
        module_items.push(SyntaxModuleItem { key, symbol: name.into() });
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("canonical Assert helpers lower through syntax ISLE");
    for name in items {
        assert!(
            artifact.functions.iter().any(|function| function.name == name),
            "expected CLIF for {name}, got {:?}",
            artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
        );
    }
    assert!(
        artifact.extern_imports.iter().any(|import| import.symbol == "panic_str"),
        "Assert helpers must still import authorized panic_str"
    );
}

#[test]
fn canonical_foundation_assert_equal_specialization_lowers_through_syntax_isle() {
    let mut db = Box::new(BeskidDatabase::default());
    let assert_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("compiler-owned Assert path");
    let assert_source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source")
        .source;
    let directory = tempfile::tempdir().expect("project").keep();
    let main_path = directory.join("Main.bd");
    let main_source = "use Testing.Assert; unit Main() { Assert.Equal(1_i64, 1_i64, \"\"); }";
    std::fs::write(&main_path, main_source).expect("main source");
    // Prefer the compiler-owned Assert identity so panic_str authority remains available.
    let assert_program =
        parse_program_with_source_name(assert_path.to_str().unwrap(), &assert_source).expect("assert parse");
    let main_program = parse_program_with_source_name(main_path.to_str().unwrap(), main_source).expect("main parse");
    let main_unit = SourceUnitId::new(&*db, main_path.clone());
    let assert_unit = SourceUnitId::new(&*db, assert_path.clone());
    let generation = SyntaxGenerationId(97);
    let source_root = assert_path.ancestors().nth(2).expect("foundation src").to_path_buf();
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        main_path.clone(),
        "beskid-foundation".into(),
        "assert-equal-specialization".into(),
    );
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Main".into(),
                path: main_path,
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
                path: assert_path,
                source: assert_source,
                program: assert_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
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
    .expect("typed Assert+Main program");
    let main_root = AstNodeKey { unit: main_unit, generation, node: AstNodeId(0) };
    let assert_root = AstNodeKey { unit: assert_unit, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([main_root, assert_root]), target, manifest)
        .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");

    let mut module_items = Vec::new();
    for (root, name) in [
        (assert_root, "trigger_failure"),
        (assert_root, "Fail"),
        (assert_root, "fail_with_because"),
        (assert_root, "Equal"),
        (main_root, "Main"),
    ] {
        let key = find_function_definitions(input.database(), root)
            .into_iter()
            .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
            .unwrap_or_else(|| panic!("expected {name}"));
        module_items.push(SyntaxModuleItem { key, symbol: name.into() });
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("Assert.Equal specialization lowers through syntax ISLE");
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")),
        "expected specialized Equal CLIF, got {:?}",
        artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn canonical_foundation_string_len_lowers_through_syntax_isle() {
    let mut db = Box::new(BeskidDatabase::default());
    let foundation_src = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("compiler-owned Assert path")
        .parent()
        .expect("Testing/")
        .parent()
        .expect("foundation src")
        .to_path_buf();
    // `Core/String/String.bd` is a hub that re-exports `Core.String.Core`, so its bodies are
    // cross-unit delegations. The leaf helpers this test lowers live in the `Core` submodule.
    let source_path = foundation_src.join("Core/String/Core.bd");
    let source = std::fs::read_to_string(&source_path).expect("read Core.String.Core");
    let source_root = foundation_src;
    let program =
        parse_program_with_source_name(source_path.to_str().unwrap(), &source).expect("parse Core.String.Core");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        source_root.clone(),
        source_path.clone(),
        "beskid-foundation".into(),
        "compiler-owned-foundation-string".into(),
    );
    let generation = SyntaxGenerationId(96);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots { host: RootEntry { dependency_name: None, source_root }, dependencies: Vec::new() },
        Arc::new(vec![SourceUnit { logical_name: "Core/String/Core.bd".into(), path: source_path, source, program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed Core.String.Core program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("generation-safe Core.String.Core input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    // Leaf helpers that exercise dispatch builtins and string indexing without pulling the
    // full String.bd call graph (Contains -> IndexOfFrom -> while/ByteAt). Only `Len` is a
    // true leaf (calls __str_len directly); `IsEmpty` and `ByteAt` delegate to Core.bd
    // functions and need the full module graph importer.
    let key = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Len"))
        .expect("Core.String Len");
    let module_items = vec![SyntaxModuleItem { key, symbol: "Len".into() }];
    lower_syntax_program(&input, isa.as_ref(), &module_items).expect("Core.String Len lowers through syntax ISLE");
}

#[test]
fn copied_foundation_assert_source_cannot_receive_panic_authority() {
    let mut db = BeskidDatabase::default();
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let directory = tempfile::tempdir().expect("copied Foundation project").keep();
    let source_path = directory.join(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH);
    std::fs::create_dir_all(source_path.parent().expect("Assert parent")).expect("create copied Assert parent");
    std::fs::write(&source_path, &source.source).expect("write copied Assert source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse copied Foundation Assert source");
    let generation = SyntaxGenerationId(95);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program: program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let project =
        ProjectSession::new(&db, directory, source_path.clone(), "copied-foundation".into(), "copied-assert".into());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("copied source remains an ordinary syntax program");
    assert!(typed.corelib_service_capability.is_none());

    let trigger_failure = SyntaxIndex::from_program(&program, generation)
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: SourceUnitId::new(&db, source_path.clone()), generation, node })
        .find(|key| {
            call_lowering(&db, *key)
                .ok()
                .flatten()
                .is_some_and(|lowering| matches!(lowering, beskid_queries::CallLowering::Dynamic))
        })
        .expect("copied panic spelling remains dynamic");
    assert!(matches!(
        call_lowering(&db, trigger_failure).expect("copied call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_foundation_assert_source_cannot_receive_panic_authority() {
    let mut db = BeskidDatabase::default();
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let directory = tempfile::tempdir().expect("symlinked Foundation project").keep();
    let source_path = directory.join(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH);
    std::fs::create_dir_all(source_path.parent().expect("Assert parent")).expect("create symlinked Assert parent");
    let compiler_owned_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("compiler-owned Assert path");
    std::os::unix::fs::symlink(&compiler_owned_path, &source_path)
        .expect("link compiler-owned Assert source into user project");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse symlinked Foundation Assert source");
    let generation = SyntaxGenerationId(96);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            path: source_path.clone(),
            source: source.source,
            program: program.clone(),
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
    ));
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let project = ProjectSession::new(
        &db,
        directory,
        source_path.clone(),
        "symlinked-foundation".into(),
        "symlinked-assert".into(),
    );
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("symlinked source remains an ordinary syntax program");
    assert!(typed.corelib_service_capability.is_none());

    let trigger_failure = SyntaxIndex::from_program(&program, generation)
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: SourceUnitId::new(&db, source_path.clone()), generation, node })
        .find(|key| {
            call_lowering(&db, *key)
                .ok()
                .flatten()
                .is_some_and(|lowering| matches!(lowering, beskid_queries::CallLowering::Dynamic))
        })
        .expect("symlinked panic spelling remains dynamic");
    assert!(matches!(
        call_lowering(&db, trigger_failure).expect("symlinked call lowering"),
        Some(beskid_queries::CallLowering::Dynamic)
    ));
}
