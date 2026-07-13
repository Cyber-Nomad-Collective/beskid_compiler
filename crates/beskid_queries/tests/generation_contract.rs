use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ModHostSyntaxGenerationId, ProjectSession,
    SemanticQueryUnavailable, SourceUnitId, SyntaxGenerationId, TypedProgram, call_lowering,
    cast_intents, control_flow, item_signature, node_type, resolved_item, resolved_local,
    runtime_intrinsic,
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

#[cfg(unix)]
#[test]
fn symlink_parent_traversal_is_not_lexically_conflated() {
    use std::os::unix::fs::symlink;

    let db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("temp directory");
    let physical = directory.path().join("physical");
    let child = physical.join("child");
    let linked_child = directory.path().join("linked-child");
    std::fs::create_dir_all(&child).expect("create physical child");
    symlink(&child, &linked_child).expect("create child symlink");

    let traversed = SourceUnitId::new(&db, linked_child.join("..").join("New.bd"));
    let lexical = SourceUnitId::new(&db, directory.path().join("New.bd"));
    assert_ne!(traversed, lexical);
    assert_eq!(
        traversed.path(&db),
        &physical
            .canonicalize()
            .expect("canonical physical directory")
            .join("New.bd")
    );

    std::fs::write(physical.join("New.bd"), "i32 Main() { return 0; }").expect("create source");
    let after_creation = SourceUnitId::new(&db, linked_child.join("..").join("New.bd"));
    assert_eq!(traversed, after_creation);
}

#[test]
fn stale_generation_has_no_semantic_facts() {
    let mut db = BeskidDatabase::default();
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
    let authority = db.ensure_syntax_unit(typed.entry, typed.generation);
    let current = AstNodeKey {
        unit: entry,
        generation: typed.generation,
        node: AstNodeId(0),
    };
    let stale = AstNodeKey {
        generation: SyntaxGenerationId(3),
        ..current
    };
    assert!(db.syntax_unit(typed.entry) == Some(authority));

    // Task 1A declares the Salsa interfaces; Task 2 populates their semantic facts.
    assert_eq!(resolved_item(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(resolved_local(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(node_type(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(call_lowering(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(cast_intents(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(control_flow(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(item_signature(&db, current), Err(SemanticQueryUnavailable));
    assert_eq!(
        runtime_intrinsic(&db, current),
        Err(SemanticQueryUnavailable)
    );

    assert_eq!(resolved_item(&db, stale), Ok(None));
    assert_eq!(resolved_local(&db, stale), Ok(None));
    assert_eq!(node_type(&db, stale), Ok(None));
    assert_eq!(call_lowering(&db, stale), Ok(None));
    assert_eq!(cast_intents(&db, stale), Ok(None));
    assert_eq!(control_flow(&db, stale), Ok(None));
    assert_eq!(item_signature(&db, stale), Ok(None));
    assert_eq!(runtime_intrinsic(&db, stale), Ok(None));

    let unregistered = AstNodeKey {
        unit: SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Unregistered.bd")),
        generation: typed.generation,
        node: AstNodeId(0),
    };
    assert_eq!(resolved_item(&db, unregistered), Ok(None));

    let same_authority = db
        .update_syntax_unit(typed.entry, SyntaxGenerationId(5))
        .expect("registered syntax unit");
    assert!(same_authority == authority);
    assert_eq!(resolved_item(&db, current), Ok(None));
    let current_after_update = AstNodeKey {
        generation: SyntaxGenerationId(5),
        ..current
    };
    assert_eq!(
        resolved_item(&db, current_after_update),
        Err(SemanticQueryUnavailable)
    );
}

#[test]
fn syntax_generation_names_are_unambiguous() {
    let db = BeskidDatabase::default();
    let syntax = SyntaxGenerationId(9);
    let mod_host = ModHostSyntaxGenerationId::new(&db, "Main.bd".to_string(), 9);

    assert_eq!(syntax.0, mod_host.generation(&db));
}
