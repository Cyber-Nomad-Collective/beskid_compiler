pub(super) use std::path::PathBuf;
pub(super) use std::sync::Arc;

pub(super) use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
pub(super) use beskid_abi::runtime_source::{
    CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH, CANONICAL_BOOTSTRAP_SOURCE_PATH, CANONICAL_CORELIB_ARGS_SOURCE_PATH,
    CANONICAL_SCHEDULER_CORE_SOURCE_PATH, canonical_corelib_service_capability, canonical_corelib_service_source_path,
    canonical_corelib_service_sources, canonical_runtime_intrinsic_capability, canonical_runtime_sources,
};
pub(super) use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, ProgramAssembly,
};
pub(super) use beskid_analysis::services::parse_program_with_source_name;
pub(super) use beskid_codegen::{CodegenInput, CodegenInputError, SyntaxNodeFacts};
pub(super) use beskid_isle::NodeFacts;
pub(super) use beskid_isle::callee::DirectCallee;
pub(super) use beskid_isle::syntax_types::CallKind;
pub(super) use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, IndexedNodeKind, ProjectSession, SemanticTypeId, SourceUnitId,
    SyntaxGenerationId, TypedProgram, build_canonical_runtime_typed_program, build_typed_program,
    build_typed_program_with_corelib_services, call_abi_signature, call_lowering, child_nodes, item_name, node_kind,
    node_span, primitive_numeric_conversion,
};

pub(super) fn input_fixture() -> (BeskidDatabase, TypedProgram, AstNodeKey, TargetMetadata) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project").keep();
    let source_path = directory.join("Main.bd");
    let source = "i32 Main() { native_word_from_pointer(0); return 7; }";
    std::fs::write(&source_path, source).expect("source");
    let program = parse_program_with_source_name(source_path.to_str().unwrap(), source).expect("parse source");
    let entry = SourceUnitId::new(&db, source_path.clone());
    let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit { logical_name: "Main".into(), path: source_path, source: source.into(), program }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false, generation
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).expect("typed program");
    let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("target");
    (db, typed, root, target)
}

pub(super) fn find_node_matching(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    target: IndexedNodeKind,
    predicate: impl Fn(AstNodeKey) -> bool + Copy,
) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(target) && predicate(key) {
        return Some(key);
    }
    child_nodes(db, key)
        .ok()
        .flatten()?
        .iter()
        .copied()
        .find_map(|child| find_node_matching(db, child, target, predicate))
}

pub(super) fn linux_target() -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target")
}

pub(super) fn find_node(db: &dyn beskid_queries::Db, key: AstNodeKey, target: IndexedNodeKind) -> Option<AstNodeKey> {
    if node_kind(db, key).ok().flatten() == Some(target) {
        return Some(key);
    }
    child_nodes(db, key).ok().flatten()?.iter().copied().find_map(|child| find_node(db, child, target))
}
