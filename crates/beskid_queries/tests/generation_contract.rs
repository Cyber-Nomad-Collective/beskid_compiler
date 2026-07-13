use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ModHostSyntaxGenerationId, ProjectSession, SourceUnitId,
    SyntaxGenerationId, SyntaxUnitInput, TypedProgram, call_lowering, cast_intents, control_flow,
    item_signature, node_type, resolved_item, resolved_local, runtime_intrinsic,
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
    assert!(main.path(&db).is_absolute());

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

#[cfg(unix)]
#[test]
fn missing_source_under_symlink_keeps_identity_after_creation() {
    use std::os::unix::fs::symlink;

    let db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("temp directory");
    let real = directory.path().join("real");
    let linked = directory.path().join("linked");
    std::fs::create_dir(&real).expect("create real directory");
    symlink(&real, &linked).expect("create directory symlink");
    let missing_through_link = linked.join("New.bd");

    let before_creation = SourceUnitId::new(&db, missing_through_link.clone());
    std::fs::write(real.join("New.bd"), "i32 Main() { return 0; }").expect("create source");
    let after_creation = SourceUnitId::new(&db, missing_through_link);

    assert_eq!(before_creation, after_creation);
    assert_eq!(
        before_creation.path(&db),
        &real
            .canonicalize()
            .expect("canonical real directory")
            .join("New.bd")
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
    let authority = SyntaxUnitInput::new(&db, typed.entry, typed.generation);
    let current = AstNodeKey {
        unit: entry,
        generation: typed.generation,
        node: AstNodeId(0),
    };
    let stale = AstNodeKey {
        generation: SyntaxGenerationId(3),
        ..current
    };
    assert!(authority.accepts_key(&db, current));
    assert!(!authority.accepts_key(&db, stale));

    // Task 1A declares the Salsa interfaces; Task 2 populates their semantic facts.
    assert!(resolved_item(&db, authority, current).is_none());
    assert!(resolved_local(&db, authority, current).is_none());
    assert!(node_type(&db, authority, current).is_none());
    assert!(call_lowering(&db, authority, current).is_none());
    assert!(cast_intents(&db, authority, current).is_none());
    assert!(control_flow(&db, authority, current).is_none());
    assert!(item_signature(&db, authority, current).is_none());
    assert!(runtime_intrinsic(&db, authority, current).is_none());

    assert!(resolved_item(&db, authority, stale).is_none());
    assert!(resolved_local(&db, authority, stale).is_none());
    assert!(node_type(&db, authority, stale).is_none());
    assert!(call_lowering(&db, authority, stale).is_none());
    assert!(cast_intents(&db, authority, stale).is_none());
    assert!(control_flow(&db, authority, stale).is_none());
    assert!(item_signature(&db, authority, stale).is_none());
    assert!(runtime_intrinsic(&db, authority, stale).is_none());
}

#[test]
fn syntax_generation_names_are_unambiguous() {
    let db = BeskidDatabase::default();
    let syntax = SyntaxGenerationId(9);
    let mod_host = ModHostSyntaxGenerationId::new(&db, "Main.bd".to_string(), 9);

    assert_eq!(syntax.0, mod_host.generation(&db));
}
