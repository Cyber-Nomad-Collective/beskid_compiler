use super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase,
    CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH, CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, NodeKind, ProgramAssembly, ProjectSession, RootEntry, SourceUnit,
    SourceUnitId, SyntaxGenerationId, SyntaxIndex, SyntaxModuleItem, TargetMetadata,
    build_typed_program_with_corelib_services, call_abi_signature, call_lowering, canonical_corelib_service_capability,
    canonical_corelib_service_source_path, canonical_foundation_assert_fixture, enum_layout, find_call_expression,
    find_corelib_service_call, find_definition_of_kind, find_function_definitions, isa, item_fixture_with_root,
    item_name, lower_syntax_program, parse_program_with_source_name, settings,
};

fn network_internal_panic_fixture(
    copied: bool,
    include_receive_dependencies: bool,
) -> (CodegenInput<'static>, Arc<dyn cranelift_codegen::isa::TargetIsa>, AstNodeKey) {
    use beskid_abi::runtime_source::CANONICAL_NETWORK_INTERNAL_SOURCE_PATH;

    let mut db = Box::new(BeskidDatabase::default());
    let canonical_path = canonical_corelib_service_source_path(CANONICAL_NETWORK_INTERNAL_SOURCE_PATH)
        .expect("canonical Network.Internal path");
    let source_root = canonical_path.ancestors().nth(2).expect("network source root").to_path_buf();
    let copied_root = copied.then(|| tempfile::tempdir().expect("untrusted Network copy").keep());
    let mut units = Vec::new();
    let slice_path = canonical_corelib_service_source_path("Core/Bytes/Slice.bd").unwrap();
    let foundation_root = slice_path.ancestors().nth(3).unwrap().to_path_buf();
    let mut sources = vec!["Network/Internal.bd", "Network/Errors.bd"];
    if include_receive_dependencies {
        sources.extend(["Core/Bytes/Slice.bd", "Core/Collections/Array.bd", "Core/Collections/Array/ArrayIter.bd"]);
    }
    for relative in sources {
        let original =
            if relative.starts_with("Core/") { foundation_root.join(relative) } else { source_root.join(relative) };
        let source = std::fs::read_to_string(&original).expect("canonical Network source");
        let path = if let Some(root) = &copied_root {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).expect("copied source directory");
            std::fs::write(&path, &source).expect("identical copied source");
            path
        } else {
            original
        };
        let program = parse_program_with_source_name(path.to_str().unwrap(), &source).expect("Network syntax");
        units.push(SourceUnit { logical_name: relative.into(), origin_path: path.clone(), path, source, program });
    }
    let source_root = copied_root.unwrap_or(source_root);
    let entry_path = units[0].path.clone();
    let generation = SyntaxGenerationId(103);
    let roots: Arc<[AstNodeKey]> = units
        .iter()
        .map(|unit| AstNodeKey { unit: SourceUnitId::new(&*db, unit.path.clone()), generation, node: AstNodeId(0) })
        .collect();
    let root = roots[0];
    let project =
        ProjectSession::new(&*db, source_root.clone(), entry_path, "network-panic".into(), "source-authority".into());
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root },
            dependencies: vec![RootEntry { dependency_name: Some("foundation".into()), source_root: foundation_root }],
        },
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
        canonical_corelib_service_capability(&manifest).expect("canonical service capability"),
    )
    .expect("typed Network source");
    let input = CodegenInput::new(Box::leak(db), typed, roots, target, manifest).expect("Network codegen input");
    let isa =
        isa::lookup_by_name("x86_64").expect("ISA").finish(settings::Flags::new(settings::builder())).expect("flags");
    (input, isa, root)
}

#[test]
fn canonical_network_internal_panic_lowers_with_manifest_backed_never_abi() {
    let (input, isa, root) = network_internal_panic_fixture(false, false);
    let error_mapper = super::support::named_function(&input, root, "Error");
    let artifact =
        lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: error_mapper, symbol: "Error".into() }])
            .expect("canonical Network error mapper lowers through its scoped panic service");
    let call = find_corelib_service_call(input.database(), error_mapper, "__panic_str").expect("authorized panic");
    assert!(matches!(
        call_lowering(input.database(), call).expect("panic lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.symbol == "beskid_trap_message"
    ));
    assert_eq!(
        call_abi_signature(input.database(), call).expect("panic ABI").expect("signature").result,
        beskid_queries::SemanticTypeId::NEVER
    );
    assert!(artifact.extern_imports.iter().any(|import| import.symbol == "beskid_trap_message"));
}

#[test]
fn canonical_network_receive_owns_its_length_invariant_and_lowers() {
    let (input, isa, root) = network_internal_panic_fixture(false, true);
    let receive = super::support::named_function(&input, root, "Receive");
    find_corelib_service_call(input.database(), receive, "__panic_str")
        .expect("the private receive bridge must validate its native completion length");
    find_corelib_service_call(input.database(), receive, "__network_receive").expect("same source owns receive ABI");
    let length = super::support::named_function(&input, input.roots()[2], "Len");
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: receive, symbol: "Receive".into() },
            SyntaxModuleItem { key: length, symbol: "Len".into() },
        ],
    )
    .expect("canonical receive and its invariant lower under one source-scoped authority");
}

#[test]
fn copied_network_internal_panic_remains_unauthorized() {
    let (input, isa, root) = network_internal_panic_fixture(true, false);
    let error_mapper = super::support::named_function(&input, root, "Error");
    assert!(find_corelib_service_call(input.database(), error_mapper, "__panic_str").is_none());
    let error =
        lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: error_mapper, symbol: "Error".into() }])
            .expect_err("identical untrusted Network source must not gain panic authority");
    assert!(error.to_string().contains("MissingRuleOrFact"), "{error}");
}

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
        Arc::new(vec![SourceUnit {
            logical_name: "Core/Output/Output.bd".into(),
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
        artifact.extern_imports.iter().any(|import| import.symbol == "beskid_trap_message"),
        "Assert helpers must still import the authorized trap-message adapter"
    );
}

#[test]
fn canonical_foundation_assert_collect_garbage_imports_gc_collect() {
    let (input, isa, root) = canonical_foundation_assert_fixture();
    let collect_garbage = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("CollectGarbage"))
        .expect("canonical Assert CollectGarbage");
    let call = find_corelib_service_call(input.database(), collect_garbage, "__gc_collect")
        .expect("canonical Assert collection call");
    assert!(matches!(
        call_lowering(input.database(), call).expect("collection lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__gc_collect" && service.symbol == "gc_collect"
    ));

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: collect_garbage, symbol: "CollectGarbage".into() }],
    )
    .expect("canonical Assert.CollectGarbage lowers through syntax ISLE");
    assert!(
        artifact.extern_imports.iter().any(|import| import.symbol == "gc_collect"),
        "Assert.CollectGarbage must emit the authorized gc_collect import"
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
                origin_path: main_path.clone(),
                path: main_path,
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
                origin_path: assert_path.clone(),
                path: assert_path,
                source: assert_source,
                program: assert_program,
            },
        ]),
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
    let source_path = canonical_corelib_service_source_path(CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH)
        .expect("compiler-owned Core.String.Core path");
    let source = std::fs::read_to_string(&source_path).expect("read Core.String.Core");
    let source_root = source_path.ancestors().nth(3).expect("foundation src").to_path_buf();
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
        Arc::new(vec![SourceUnit {
            logical_name: "Core/String/Core.bd".into(),
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
    .expect("typed Core.String.Core program");
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
    let call = find_corelib_service_call(input.database(), key, "__str_len").expect("authorized __str_len call");
    assert!(matches!(
        call_lowering(input.database(), call).expect("string service lowering"),
        Some(beskid_queries::CallLowering::CorelibService(service))
            if service.name == "__str_len" && service.symbol == "str_len"
    ));
    let signature = call_abi_signature(input.database(), call)
        .expect("string service ABI query")
        .expect("string service ABI signature");
    assert_eq!(signature.parameters.as_ref(), [beskid_queries::SemanticTypeId::POINTER]);
    assert_eq!(signature.result, beskid_queries::SemanticTypeId::WORD);
    let module_items = vec![SyntaxModuleItem { key, symbol: "Len".into() }];
    lower_syntax_program(&input, isa.as_ref(), &module_items).expect("Core.String Len lowers through syntax ISLE");
}

#[test]
fn copied_foundation_string_source_cannot_receive_string_service_authority() {
    let mut db = BeskidDatabase::default();
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH)
        .expect("embedded Core.String.Core source")
        .source;
    let directory = tempfile::tempdir().expect("copied Foundation project").keep();
    let source_path = directory.join(CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH);
    std::fs::create_dir_all(source_path.parent().expect("String source parent"))
        .expect("create copied String source parent");
    std::fs::write(&source_path, &source).expect("write copied String source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source)
        .expect("parse copied Core.String.Core source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let generation = SyntaxGenerationId(98);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_STRING_CORE_SOURCE_PATH.into(),
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
    let manifest = AbiManifestV5::canonical_runtime(target);
    let project = ProjectSession::new(&db, directory, source_path, "copied-foundation".into(), "copied-string".into());
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("Corelib service authority"),
    )
    .expect("copied String source remains an ordinary syntax program");
    assert!(typed.corelib_service_capability.is_none());

    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let len = find_function_definitions(&db, root)
        .into_iter()
        .find(|key| item_name(&db, *key).ok().flatten().as_deref() == Some("Len"))
        .expect("copied Core.String Len");
    assert!(
        find_corelib_service_call(&db, len, "__str_len").is_none(),
        "identical String source at an untrusted physical path must not acquire service authority"
    );
}

#[test]
fn copied_and_altered_foundation_assert_source_cannot_receive_runtime_service_authority() {
    let mut db = BeskidDatabase::default();
    let source = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|source| source.logical_path == CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH)
        .expect("embedded Foundation Assert source");
    let altered_source = format!("{}\n// user-controlled alteration\n", source.source);
    let directory = tempfile::tempdir().expect("copied Foundation project").keep();
    let source_path = directory.join(CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH);
    std::fs::create_dir_all(source_path.parent().expect("Assert parent")).expect("create copied Assert parent");
    std::fs::write(&source_path, &altered_source).expect("write copied and altered Assert source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &altered_source)
        .expect("parse copied Foundation Assert source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let generation = SyntaxGenerationId(95);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_FOUNDATION_ASSERT_SOURCE_PATH.into(),
            origin_path: source_path.clone(),
            path: source_path.clone(),
            source: altered_source,
            program: program.clone(),
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

    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let definitions = find_function_definitions(&db, root);
    let trigger_failure = definitions
        .iter()
        .copied()
        .find(|key| item_name(&db, *key).ok().flatten().as_deref() == Some("trigger_failure"))
        .expect("copied Assert trigger_failure");
    assert!(
        find_corelib_service_call(&db, trigger_failure, "__panic_str").is_none(),
        "a copied and altered Assert source must not retain panic authority"
    );
    let collect_garbage = definitions
        .into_iter()
        .find(|key| item_name(&db, *key).ok().flatten().as_deref() == Some("CollectGarbage"))
        .expect("copied Assert CollectGarbage");
    assert!(
        find_corelib_service_call(&db, collect_garbage, "__gc_collect").is_none(),
        "a copied and altered Assert source must not acquire collection authority"
    );
    let collection_call = find_call_expression(&db, collect_garbage).expect("copied __gc_collect call");
    let error = call_lowering(&db, collection_call)
        .expect_err("an unauthorized raw collection call must remain semantically unavailable");
    assert!(error.is_unavailable(), "unauthorized collection call must fail closed: {error:?}");
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
            origin_path: source_path.clone(),
            path: source_path.clone(),
            source: source.source,
            program: program.clone(),
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
