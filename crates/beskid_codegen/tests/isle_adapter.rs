use std::sync::Arc;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit,
    SyntaxProgramAssembly,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_codegen::{CodegenInput, emit_isle_expression, emit_isle_item};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_typed_program, child_nodes, literal_fact, node_kind,
};
use cranelift_codegen::ir::types;
use cranelift_codegen::isa;
use cranelift_codegen::settings;

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
    let assembly = Arc::new(SyntaxProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
    });
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
    let assembly = Arc::new(SyntaxProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
    });
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
    let assembly = Arc::new(SyntaxProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: directory,
            },
            dependencies: Vec::new(),
        },
        units: Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            path: source_path,
            source: source.into(),
            program,
        }]),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
    });
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

fn find_function_definition(db: &BeskidDatabase, key: AstNodeKey) -> Option<AstNodeKey> {
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
