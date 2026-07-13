use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::SyntaxGenerationId;
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, CallLowering, CastIntent, ControlFlow, ItemSignature,
    ProjectSession, ResolvedItem, ResolvedLocal, RuntimeIntrinsic, SemanticFacts,
    SemanticFactsInput, SemanticTypeId, SourceUnitId, TypedProgram, call_lowering, cast_intents,
    control_flow, item_signature, node_type, resolved_item, resolved_local, runtime_intrinsic,
};

fn empty_assembly() -> Arc<ProgramAssembly> {
    Arc::new(ProgramAssembly {
        roots: EffectiveCompilationRoots {
            host: RootEntry {
                dependency_name: None,
                source_root: PathBuf::from("/tmp/project/src"),
            },
            dependencies: Vec::new(),
        },
        units: Arc::new(Vec::new()),
        hir_units: Arc::new(Vec::new()),
        entry_index: 0,
        discovery: AssemblyDiscovery::ImportClosure,
        module_index: Arc::new(ModuleIndex::empty()),
        has_std_dependency: false,
    })
}

#[test]
fn source_units_are_interned_by_path_and_do_not_collide() {
    let db = BeskidDatabase::default();
    let main_path = PathBuf::from("/tmp/project/src/Main.bd");
    let other_path = PathBuf::from("/tmp/project/src/Other.bd");

    let main = SourceUnitId::new(&db, main_path.clone());
    let main_again = SourceUnitId::new(&db, main_path.clone());
    let other = SourceUnitId::new(&db, other_path);

    assert_eq!(main, main_again);
    assert_ne!(main, other);
    assert_eq!(main.path(&db), &main_path);

    let node = AstNodeId(7);
    let generation = SyntaxGenerationId(11);
    assert_ne!(
        AstNodeKey {
            unit: main,
            generation,
            node,
        },
        AstNodeKey {
            unit: other,
            generation,
            node,
        }
    );
}

#[test]
fn source_unit_interning_canonicalizes_path_aliases() {
    let db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("temp directory");
    let source = directory.path().join("Main.bd");
    std::fs::write(&source, "i32 Main() { return 0; }").expect("write source");
    let aliased = directory.path().join(".").join("Main.bd");

    let canonical = SourceUnitId::new(&db, source.canonicalize().expect("canonical source"));
    let through_alias = SourceUnitId::new(&db, aliased);

    assert_eq!(canonical, through_alias);
    assert_eq!(
        canonical.path(&db),
        &source.canonicalize().expect("canonical source")
    );
}

#[test]
fn stale_generation_has_no_semantic_facts() {
    let db = BeskidDatabase::default();
    let entry_path = PathBuf::from("/tmp/project/src/Main.bd");
    let entry = SourceUnitId::new(&db, entry_path.clone());
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry_path,
        "App".to_string(),
        "lock".to_string(),
    );
    let typed = TypedProgram {
        project,
        entry,
        generation: SyntaxGenerationId(4),
        assembly: empty_assembly(),
    };
    let current = AstNodeKey {
        unit: entry,
        generation: typed.generation,
        node: AstNodeId(0),
    };
    let stale = AstNodeKey {
        generation: SyntaxGenerationId(3),
        ..current
    };
    let type_id = SemanticTypeId(17);
    let mut seeded = SemanticFacts::default();
    seeded.resolved_items.insert(
        current,
        ResolvedItem {
            declaration: current,
        },
    );
    seeded.resolved_locals.insert(
        current,
        ResolvedLocal {
            declaration: current,
        },
    );
    seeded.node_types.insert(current, type_id);
    seeded.call_lowerings.insert(current, CallLowering::Dynamic);
    seeded.cast_intents.insert(
        current,
        Arc::from([CastIntent {
            from: type_id,
            to: SemanticTypeId(18),
        }]),
    );
    seeded.control_flow.insert(
        current,
        ControlFlow {
            may_fall_through: true,
        },
    );
    seeded.item_signatures.insert(
        current,
        ItemSignature {
            parameters: Arc::from([type_id]),
            result: type_id,
        },
    );
    seeded
        .runtime_intrinsics
        .insert(current, RuntimeIntrinsic(5));
    let facts = SemanticFactsInput::new(&db, Arc::new(seeded));

    assert!(resolved_item(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(resolved_local(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(node_type(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(call_lowering(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(cast_intents(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(control_flow(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(item_signature(&db, facts, typed.entry, typed.generation, current).is_some());
    assert!(runtime_intrinsic(&db, facts, typed.entry, typed.generation, current).is_some());

    assert!(resolved_item(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(resolved_local(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(node_type(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(call_lowering(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(cast_intents(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(control_flow(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(item_signature(&db, facts, typed.entry, typed.generation, stale).is_none());
    assert!(runtime_intrinsic(&db, facts, typed.entry, typed.generation, stale).is_none());
}
