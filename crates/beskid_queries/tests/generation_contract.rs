use std::path::PathBuf;
use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, ModHostSyntaxGenerationId, ProjectSession,
    SemanticError, SourceUnitId, SyntaxGenerationId, TypedProgram, call_lowering, cast_intents,
    control_flow, item_signature, node_type, resolved_item, resolved_local, runtime_intrinsic,
};

fn assert_unavailable<T>(result: Result<Option<T>, SemanticError>) {
    let error = match result {
        Ok(_) => panic!("current unported semantic query must fail explicitly"),
        Err(error) => error,
    };
    assert!(error.is_unavailable(), "{error:?}");
}

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
    db.ensure_file_text(
        typed.entry.path(&db).clone(),
        "i32 Main() { return 0; }".to_string(),
    );
    let authority = db
        .ensure_syntax_unit(typed.project, typed.entry, typed.generation)
        .expect("syntax registration");
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

    assert_unavailable(resolved_item(&db, current));
    assert_unavailable(resolved_local(&db, current));
    assert_unavailable(node_type(&db, current));
    assert_eq!(call_lowering(&db, current), Ok(None));
    assert_unavailable(cast_intents(&db, current));
    assert_eq!(control_flow(&db, current), Ok(None));
    assert_eq!(item_signature(&db, current), Ok(None));
    assert_unavailable(runtime_intrinsic(&db, current));

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
        .update_syntax_source(
            typed.project,
            typed.entry,
            SyntaxGenerationId(5),
            "i32 Main() { return 1; }".to_string(),
        )
        .expect("registered syntax edit");
    assert!(same_authority == authority);
    assert_eq!(resolved_item(&db, current), Ok(None));
    let current_after_update = AstNodeKey {
        generation: SyntaxGenerationId(5),
        ..current
    };
    assert_unavailable(resolved_item(&db, current_after_update));
}

#[test]
fn unchanged_ensure_is_idempotent_without_parse_or_index_rebuild() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Stable.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    db.ensure_file_text(
        entry.path(&db).clone(),
        "i32 Main() { return 0; }".to_string(),
    );

    let first = db
        .ensure_syntax_unit(project, entry, SyntaxGenerationId(1))
        .expect("first registration");
    let second = db
        .ensure_syntax_unit(project, entry, SyntaxGenerationId(1))
        .expect("idempotent registration");

    assert!(first == second);
    assert_eq!(db.syntax_authority_counts(), (1, 1));
}

#[test]
fn generations_are_monotonic_and_bound_to_source_content() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Generation.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    db.update_syntax_source(
        project,
        entry,
        SyntaxGenerationId(4),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("initial source");

    assert!(
        db.update_syntax_source(
            project,
            entry,
            SyntaxGenerationId(4),
            "i32 Main() { return 1; }".to_string(),
        )
        .is_err(),
        "changed syntax cannot reuse a generation"
    );
    assert!(
        db.update_syntax_source(
            project,
            entry,
            SyntaxGenerationId(3),
            "i32 Main() { return 2; }".to_string(),
        )
        .is_err(),
        "generation cannot regress"
    );
    assert!(
        db.update_syntax_source(
            project,
            entry,
            SyntaxGenerationId(5),
            "i32 Main() { return 0; }".to_string(),
        )
        .is_err(),
        "unchanged syntax cannot be blindly relabeled"
    );
    assert_eq!(db.syntax_authority_counts(), (1, 1));
}

#[test]
fn source_fingerprint_cannot_be_resurrected_in_a_later_generation() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/History.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let source_a = "i32 Main() { return 1; }";
    let source_b = "i32 Main() { return 2; }";
    db.update_syntax_source(project, entry, SyntaxGenerationId(4), source_a.to_string())
        .expect("generation A");
    db.update_syntax_source(project, entry, SyntaxGenerationId(5), source_b.to_string())
        .expect("generation B");

    assert!(
        db.update_syntax_source(project, entry, SyntaxGenerationId(6), source_a.to_string())
            .is_err(),
        "A@g4 -> B@g5 -> A@g6 must not resurrect an old fingerprint"
    );
    assert_eq!(db.syntax_authority_counts(), (2, 2));
}

#[test]
fn trivia_only_edit_cannot_relabel_the_same_expanded_tree() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Trivia.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let compact = "i32 Main() { return 1; }";
    let spaced = "i32   Main()   {   return 1;   }";
    db.update_syntax_source(project, entry, SyntaxGenerationId(4), compact.to_string())
        .expect("initial syntax");

    assert!(
        db.update_syntax_source(project, entry, SyntaxGenerationId(5), spaced.to_string())
            .is_err(),
        "trivia-only edits must not assign a new generation to the same expanded tree"
    );
    assert_eq!(db.syntax_authority_counts(), (2, 1));
    assert_eq!(
        db.file_text(entry.path(&db))
            .expect("original file input")
            .text(&db),
        compact
    );
}

#[test]
fn source_unit_cannot_be_reassigned_between_project_sessions() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Owned.bd"));
    let first = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "First".to_string(),
        "lock-a".to_string(),
    );
    let second = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/other"),
        entry.path(&db).clone(),
        "Second".to_string(),
        "lock-b".to_string(),
    );
    db.update_syntax_source(
        first,
        entry,
        SyntaxGenerationId(1),
        "i32 Main() { return 0; }".to_string(),
    )
    .expect("first project owns source");

    assert!(
        db.update_syntax_source(
            second,
            entry,
            SyntaxGenerationId(2),
            "i32 Main() { return 1; }".to_string(),
        )
        .is_err()
    );
}

#[test]
fn expansion_error_preserves_every_macro_diagnostic() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Macros.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let error = match db.update_syntax_source(
        project,
        entry,
        SyntaxGenerationId(1),
        "unit Main() { missing_one!(1); missing_two!(2); return; }".to_string(),
    ) {
        Ok(_) => panic!("unknown macros must fail expansion"),
        Err(error) => error,
    };

    assert!(error.diagnostics().len() >= 2, "{error:?}");
    assert!(db.syntax_unit(entry).is_none());
}

#[test]
fn failed_edit_keeps_previous_source_and_syntax_authority() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Atomic.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let source = "i32 Main() { return 0; }";
    let authority = db
        .update_syntax_source(project, entry, SyntaxGenerationId(1), source.to_string())
        .expect("initial source");

    assert!(
        db.update_syntax_source(
            project,
            entry,
            SyntaxGenerationId(2),
            "not valid Beskid".to_string(),
        )
        .is_err()
    );

    assert!(db.syntax_unit(entry) == Some(authority));
    assert_eq!(
        db.file_text(entry.path(&db))
            .expect("previous file input")
            .text(&db),
        source
    );
    assert_eq!(db.syntax_authority_counts(), (2, 1));
}

#[test]
fn syntax_registration_reports_parse_failure_instead_of_inventing_empty_program() {
    let mut db = BeskidDatabase::default();
    let entry = SourceUnitId::new(&db, PathBuf::from("/tmp/project/src/Broken.bd"));
    let project = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        entry.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    db.ensure_file_text(entry.path(&db).clone(), "not valid Beskid".to_string());

    assert!(
        db.ensure_syntax_unit(project, entry, SyntaxGenerationId(1))
            .is_err()
    );
    assert!(db.syntax_unit(entry).is_none());
}

#[test]
fn syntax_generation_names_are_unambiguous() {
    let db = BeskidDatabase::default();
    let syntax = SyntaxGenerationId(9);
    let mod_host = ModHostSyntaxGenerationId::new(&db, "Main.bd".to_string(), 9);

    assert_eq!(syntax.0, mod_host.generation(&db));
}
