use std::collections::HashMap;
use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_SOURCE_PATH, canonical_runtime_intrinsic_capability,
    canonical_runtime_sources,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
    SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_codegen::{
    CodegenInput, ItemModuleImporter, emit_isle_expression, emit_isle_item,
    emit_isle_item_with_call_importer,
    module_emission::{SyntaxModuleItem, emit_syntax_program, lower_syntax_program},
    syntax_item_signature,
};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_canonical_runtime_typed_program, build_typed_program, call_lowering, child_nodes,
    item_name, literal_fact, node_kind, test_statement_nodes,
};
use cranelift_codegen::ir::types;
use cranelift_codegen::isa;
use cranelift_codegen::settings;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};

#[test]
fn parsed_syntax_root_emits_verified_isle_clif_without_hir() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { return 42; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let literal = find_integer_literal(&db, root).expect("integer literal key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");

    let function = emit_isle_expression(&input, isa.as_ref(), literal, types::I32)
        .expect("parsed expression lowers through generated ISLE");

    assert!(function.display().to_string().contains("iconst.i32 42"));
}

#[test]
fn parsed_function_body_emits_verified_isle_clif_without_lowerable() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { return 42; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let item = find_function_definition(&db, root).expect("function key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed function body lowers through generated ISLE");

    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 42"), "{clif}");
    assert!(clif.contains("return"), "{clif}");
}

#[test]
fn parsed_test_item_emits_verified_isle_clif_without_lowerable() {
    let (input, isa, root) = item_fixture_with_root("test Smoke { return; }");
    let item = find_test_definition(input.database(), root).expect("test item key");

    let statements = test_statement_nodes(input.database(), item)
        .expect("test statement query")
        .expect("test statement nodes");
    assert_eq!(statements.len(), 1);
    assert_eq!(
        node_kind(input.database(), statements[0])
            .expect("statement kind")
            .expect("statement node"),
        beskid_queries::IndexedNodeKind::ReturnStatement
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed test item lowers through generated ISLE");

    assert!(function.display().to_string().contains("return"));
}

#[test]
fn parsed_local_read_emits_verified_isle_clif_without_lowerable() {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { i32 answer = 42; return answer; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(
        &db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let item = find_function_definition(&db, root).expect("function key");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let input = CodegenInput::new(
        &db,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let flags = settings::Flags::new(settings::builder());
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(flags)
        .expect("host flags");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed local read lowers through generated ISLE");

    assert!(function.display().to_string().contains("iconst.i32 42"));
}

#[test]
fn parsed_parameter_read_materializes_the_generation_safe_local_slot() {
    let (input, isa, item) = item_fixture("i32 Identity(i32 value) { return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("parsed parameter read lowers through generated ISLE");
    let clif = function.display().to_string();
    assert!(clif.contains("function u0:0(i32) -> i32"), "{clif}");
    assert!(clif.contains("return v0"), "{clif}");
}

#[test]
fn parsed_pointer_signature_uses_the_target_pointer_type_without_hir() {
    let (input, isa, item) = item_fixture("pointer Echo(pointer value) { return value; }");

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("pointer syntax lowers through generated ISLE");
    let clif = function.display().to_string();
    assert!(clif.contains("function u0:0(i64) -> i64"), "{clif}");
    assert!(clif.contains("return v0"), "{clif}");
}

#[test]
fn parsed_transparent_generic_aggregate_uses_its_source_proven_i64_abi_signature() {
    let (input, isa, root) = item_fixture_with_root(
        "type Channel<T> { i64 handle } Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; }",
    );
    let create = find_function_definitions(input.database(), root)[0];

    let signature = syntax_item_signature(&input, isa.as_ref(), create)
        .expect("transparent aggregate source layout supplies an ABI signature");
    assert_eq!(signature.returns[0].value_type, types::I64);
}

#[test]
fn parsed_direct_call_uses_explicit_item_module_importer() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let callee = items[0];
    let caller = items[1];
    let call = find_call_expression(db, caller).expect("call syntax key");
    let beskid_queries::CallLowering::Direct(declaration) = call_lowering(db, call)
        .expect("direct-call query")
        .expect("direct call")
    else {
        panic!("expected a syntax-resolved direct call");
    };
    assert_eq!(declaration, callee);

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let signature = function_signature(isa.as_ref(), types::I32, [types::I32]);
    let imported = module
        .declare_function("AddOne", Linkage::Import, &signature)
        .expect("declare imported syntax item");
    let mut importer = ItemModuleImporter::new(
        &mut module,
        HashMap::from([(beskid_isle::DirectCallee::item(declaration), imported)]),
    );

    let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
        .expect("parsed direct call lowers through explicit module import");
    let clif = function.display().to_string();
    assert!(clif.contains("call"), "{clif}");
    assert!(clif.contains("iconst.i32 41"), "{clif}");
}

#[test]
fn parsed_program_declares_then_imports_syntax_items_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let declared = emit_syntax_program(
        &mut module,
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: items[0],
                symbol: "AddOne".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Main".into(),
            },
        ],
        Linkage::Export,
    )
    .expect("syntax items declare before their direct-call bodies lower");
    assert_eq!(declared.len(), 2);
    assert_eq!(
        module.get_name("AddOne"),
        Some(cranelift_module::FuncOrDataId::Func(declared[&items[0]]))
    );
    assert_eq!(
        module.get_name("Main"),
        Some(cranelift_module::FuncOrDataId::Func(declared[&items[1]]))
    );
}

#[test]
fn parsed_program_lowers_to_backend_artifact_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem {
                key: items[0],
                symbol: "AddOne".into(),
            },
            SyntaxModuleItem {
                key: items[1],
                symbol: "Main".into(),
            },
        ],
    )
    .expect("syntax items lower into a normal backend artifact");

    assert_eq!(artifact.functions.len(), 2);
    beskid_codegen::validate_artifact(&artifact)
        .expect("direct syntax calls resolve against artifact definitions");
    let main = artifact
        .functions
        .iter()
        .find(|function| function.name == "Main")
        .expect("Main artifact function");
    assert!(main.function.display().to_string().contains("call"));
}

#[test]
fn parsed_syntax_program_uses_the_existing_artifact_string_pool() {
    let (input, isa, root) = item_fixture_with_root("unit Main() { \"Beskid\"; return; }");
    let main = find_function_definitions(input.database(), root)[0];
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem {
            key: main,
            symbol: "Main".into(),
        }],
    )
    .expect("syntax item with a string literal lowers through the artifact pool");

    assert_eq!(artifact.string_literals.len(), 1);
    assert!(
        artifact
            .string_literals
            .values()
            .any(|bytes| bytes.as_slice() == b"Beskid")
    );
}

#[test]
fn canonical_runtime_allocation_and_root_frame_helpers_emit_verified_clif_with_manifest_imports() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let source = canonical_runtime_sources()
        .pop()
        .expect("embedded canonical runtime source");
    let source_path = directory.join("Bootstrap.bd");
    std::fs::write(&source_path, &source.source).expect("write canonical runtime source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), &source.source)
        .expect("parse canonical runtime source");
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(31);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: CANONICAL_BOOTSTRAP_SOURCE_PATH.into(),
            path: source_path.clone(),
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
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("canonical runtime syntax facts");
    let root = AstNodeKey {
        unit: SourceUnitId::new(&*db, source_path),
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([root]), target, manifest)
        .expect("canonical runtime codegen input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let items = find_function_definitions(input.database(), root);
    let selected = [
        "NativePointer",
        "SystemAllocate",
        "RootFramePrevious",
        "RootFrame",
    ];
    let module_items = selected
        .into_iter()
        .map(|name| {
            let key = items
                .iter()
                .copied()
                .find(|key| {
                    item_name(input.database(), *key).ok().flatten().as_deref() == Some(name)
                })
                .unwrap_or_else(|| panic!("canonical helper {name}"));
            SyntaxModuleItem {
                key,
                symbol: name.into(),
            }
        })
        .collect::<Vec<_>>();

    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("canonical helpers lower through the syntax-only module emitter");

    beskid_codegen::validate_artifact(&artifact)
        .expect("canonical helper imports are declared by the manifest authority");
    let imports = beskid_codegen::referenced_extern_imports(&artifact);
    assert!(
        imports
            .iter()
            .any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_system_allocate")
    );
    assert!(
        imports
            .iter()
            .any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_raw_word_load")
    );
    assert!(
        imports
            .iter()
            .any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_pointer_from_native_word")
    );
    assert!(
        imports
            .iter()
            .any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_pointer_add")
    );

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let declared = emit_syntax_program(
        &mut module,
        &input,
        isa.as_ref(),
        &module_items,
        Linkage::Export,
    )
    .expect("canonical runtime helpers define through the production module emitter");
    assert_eq!(declared.len(), module_items.len());
}

fn item_fixture(
    source: &str,
) -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let (input, isa, root) = item_fixture_with_root(source);
    let item = find_function_definition(input.database(), root).expect("function key");
    (input, isa, item)
}

fn item_fixture_with_root(
    source: &str,
) -> (
    CodegenInput<'static>,
    Arc<dyn cranelift_codegen::isa::TargetIsa>,
    AstNodeKey,
) {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source)
        .expect("parse source");
    let entry = SourceUnitId::new(&*db, source_path.clone());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "App".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(21);
    let assembly = Arc::new(SyntaxProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
    ));
    let typed =
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
    let root = AstNodeKey {
        unit: entry,
        generation,
        node: AstNodeId(0),
    };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(
        leaked,
        typed,
        Arc::from([root]),
        target.clone(),
        AbiManifestV5::canonical_runtime(target),
    )
    .expect("generation-safe input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    (input, isa, root)
}

fn function_signature(
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    result: cranelift_codegen::ir::Type,
    parameters: impl IntoIterator<Item = cranelift_codegen::ir::Type>,
) -> cranelift_codegen::ir::Signature {
    let mut signature = cranelift_codegen::ir::Signature::new(isa.default_call_conv());
    signature.params.extend(
        parameters
            .into_iter()
            .map(cranelift_codegen::ir::AbiParam::new),
    );
    signature
        .returns
        .push(cranelift_codegen::ir::AbiParam::new(result));
    signature
}

fn find_function_definitions(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Vec<AstNodeKey> {
    let mut found = Vec::new();
    if node_kind(db, key).ok().flatten()
        == Some(beskid_queries::IndexedNodeKind::FunctionDefinition)
    {
        found.push(key);
    }
    if let Some(children) = child_nodes(db, key).ok().flatten() {
        for child in children.iter().copied() {
            found.extend(find_function_definitions(db, child));
        }
    }
    found
}

fn find_call_expression(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(beskid_queries::IndexedNodeKind::CallExpression) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_call_expression(db, child))
}

fn find_function_definition(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key)
        .ok()
        .flatten()
        .is_some_and(|kind| kind == beskid_queries::IndexedNodeKind::FunctionDefinition)
    {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_function_definition(db, child))
}

fn find_test_definition(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Option<AstNodeKey> {
    if node_kind(db, key)
        .ok()
        .flatten()
        .is_some_and(|kind| kind == beskid_queries::IndexedNodeKind::TestDefinition)
    {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_test_definition(db, child))
}

fn find_integer_literal(db: &BeskidDatabase, key: AstNodeKey) -> Option<AstNodeKey> {
    if literal_fact(db, key)
        .ok()
        .flatten()
        .is_some_and(|fact| matches!(fact, beskid_queries::LiteralFact::Integer(value) if value.as_ref() == "42"))
    {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_integer_literal(db, child))
}
