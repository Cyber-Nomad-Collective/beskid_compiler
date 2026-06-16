//! Incremental invalidation tests for Salsa-backed unit queries.

use std::path::{Path, PathBuf};

use beskid_analysis::services::{SemanticSnapshot, SessionFingerprint, cached_semantic_snapshot};
use beskid_analysis::services::{
    get_or_insert_assembly, invalidate_entry_sessions, update_semantic_snapshot,
};
use beskid_queries::{
    BeskidDatabase, Db, ProjectSession, fingerprint_key, parse_and_expand_unit, record_query_hit,
    reset, semantic_snapshot, snapshot, unit_content_fingerprint, unit_hir, unit_imports,
    unit_type_surface,
};

fn fixture_source() -> String {
    "use std.io;\ni32 Main() { return 0; }".to_string()
}

fn fixture_path() -> PathBuf {
    PathBuf::from("/tmp/beskid_salsa_test_main.bd")
}

#[test]
fn semantic_snapshot_query_hits_registry() {
    invalidate_entry_sessions();
    let fp = SessionFingerprint {
        project_root: PathBuf::from("/tmp/project"),
        entry_canonical: PathBuf::from("/tmp/project/Main.bd"),
        lockfile_digest: 42,
    };
    get_or_insert_assembly(
        fp.clone(),
        beskid_analysis::projects::ProgramAssembly {
            roots: beskid_analysis::projects::EffectiveCompilationRoots {
                host: beskid_analysis::projects::RootEntry {
                    dependency_name: None,
                    source_root: PathBuf::from("/tmp/project/src"),
                },
                dependencies: Vec::new(),
            },
            units: std::sync::Arc::new(Vec::new()),
            hir_units: std::sync::Arc::new(Vec::new()),
            entry_index: 0,
            discovery: beskid_analysis::projects::AssemblyDiscovery::ImportClosure,
            module_index: std::sync::Arc::new(beskid_analysis::projects::ModuleIndex::empty()),
            has_std_dependency: false,
        },
    );
    update_semantic_snapshot(&fp, SemanticSnapshot::from_diagnostics(&[], 1, "semantic"));
    let db = BeskidDatabase::default();
    reset();
    record_query_hit();
    let key = fingerprint_key(&fp);
    assert_eq!(semantic_snapshot(&db, &key), 0);
    assert!(cached_semantic_snapshot(&fp).is_some());
}

#[test]
fn unit_fingerprint_changes_when_source_changes() {
    let path = fixture_path();
    let fp1 = unit_content_fingerprint(&path, "i32 Main() { return 0; }");
    let fp2 = unit_content_fingerprint(&path, "i32 Main() { return 1; }");
    assert_ne!(fp1, fp2);
}

#[test]
fn second_parse_hits_unit_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = fixture_path();
    let source = fixture_source();
    let fp = unit_content_fingerprint(&path, &source);
    db.ensure_file_text(path.clone(), source);

    let session = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let unit1 = parse_and_expand_unit(&db, session, path.clone());
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .source_units
            .contains_key(&fp),
        "first parse should populate unit cache"
    );
    let unit2 = parse_and_expand_unit(&db, session, path.clone());
    assert_eq!(unit1.logical_name, unit2.logical_name);
}

#[test]
fn file_edit_invalidates_unit_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = fixture_path();
    db.ensure_file_text(path.clone(), fixture_source());

    let session = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );

    let _ = unit_hir(&db, session, path.clone());
    reset();
    db.set_file_text(path.clone(), "i32 Main() { return 99; }".to_string());
    let _ = unit_hir(&db, session, path.clone());
    let (_, misses, _) = snapshot();
    assert!(misses >= 1);
}

#[test]
fn unit_imports_tracks_use_paths() {
    let mut db = BeskidDatabase::default();
    let path = fixture_path();
    db.ensure_file_text(path.clone(), fixture_source());
    let session = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let grammar = db.grammar_revision();
    let imports = unit_imports(&db, session, grammar, path);
    assert!(imports.iter().any(|import| import.contains("std.io")));
}

#[test]
fn warm_second_parse_reuses_unit_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = fixture_path();
    let source = fixture_source();
    let fp = unit_content_fingerprint(&path, &source);
    db.ensure_file_text(path.clone(), source);

    let session = ProjectSession::new(
        &db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );

    let _ = parse_and_expand_unit(&db, session, path.clone());
    let misses_after_cold = snapshot().1;

    let _ = parse_and_expand_unit(&db, session, path.clone());
    let misses_after_warm = snapshot().1;

    assert!(
        misses_after_warm <= misses_after_cold,
        "warm parse should not increase misses (cold={misses_after_cold} warm={misses_after_warm})"
    );
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .source_units
            .contains_key(&fp)
    );
}

#[test]
fn entry_resolution_with_db_populates_symbol_registry() {
    use std::path::PathBuf;

    use beskid_analysis::projects::AssemblyDiscovery;
    use beskid_analysis::services::{PrepareOptions, resolve_input};
    use beskid_queries::{
        BeskidDatabase, configure_db_for_project, entry_resolution_with_db,
    };

    let compiler_root = {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .and_then(|p| p.parent())
            .expect("compiler root")
            .to_path_buf()
    };
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_e2e_tests/fixtures/corelib_mvp");
    let main_path = fixture_root.join("Src/Main.bd");
    let _source = std::fs::read_to_string(&main_path).expect("read Main.bd");
    let project_root = fixture_root
        .canonicalize()
        .unwrap_or(fixture_root.clone());

    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&compiler_root).expect("chdir");
    let result = {
        configure_db_for_project(&project_root);
        let resolved = resolve_input(
            Some(&main_path),
            Some(&project_root),
            Some("App"),
            None,
            false,
            false,
        )
        .expect("resolve fixture");
        let mut db = BeskidDatabase::with_persistence(&project_root);
        let mut options = PrepareOptions::default();
        options.front_end.assembly_discovery = AssemblyDiscovery::ImportClosure;
        entry_resolution_with_db(&mut db, &resolved, &options)
    };
    std::env::set_current_dir(previous).expect("restore cwd");

    let shared = result.expect("entry resolution");
    assert!(
        !shared.by_symbol().is_empty(),
        "expected prefetch symbols in by_symbol"
    );
    assert!(
        shared.items.iter().any(|item| item.name == "WriteLine"),
        "expected WriteLine in entry resolution items"
    );
    assert!(
        shared
            .qualified_name(
                shared
                    .items
                    .iter()
                    .find(|i| i.name == "WriteLine")
                    .unwrap()
                    .id
            )
            .is_some(),
        "WriteLine should have registry-backed qualified name"
    );
}

#[test]
fn typed_entry_state_uses_fast_resolution_when_stale() {
    use beskid_analysis::projects::AssemblyDiscovery;
    use beskid_analysis::services::{PrepareOptions, resolve_input};
    use beskid_queries::{
        bump_file_revision, bump_typed_prepare_revision, configure_db_for_project,
        entry_resolution_with_db, fingerprint_key, is_typed_bundle_stale,
        typed_entry_state_with_db,
    };

    let compiler_root = {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .parent()
            .and_then(|p| p.parent())
            .expect("compiler root")
            .to_path_buf()
    };
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../beskid_e2e_tests/fixtures/corelib_mvp");
    let main_path = fixture_root.join("Src/Main.bd");
    let project_root = fixture_root
        .canonicalize()
        .unwrap_or(fixture_root.clone());

    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&compiler_root).expect("chdir");
    let result = {
        configure_db_for_project(&project_root);
        let resolved = resolve_input(
            Some(&main_path),
            Some(&project_root),
            Some("App"),
            None,
            false,
            false,
        )
        .expect("resolve fixture");
        let mut db = BeskidDatabase::with_persistence(&project_root);
        let mut options = PrepareOptions::default();
        options.front_end.assembly_discovery = AssemblyDiscovery::ImportClosure;
        let entry_key = resolved
            .compile_plan
            .as_ref()
            .map(|plan| {
                fingerprint_key(&beskid_analysis::services::SessionFingerprint::for_entry(
                    plan,
                    &resolved.source_path,
                ))
            })
            .expect("entry key");
        bump_file_revision(&mut db, &entry_key);
        assert!(is_typed_bundle_stale(&db, &entry_key));

        let state = typed_entry_state_with_db(&mut db, &resolved, &options, None)
            .expect("typed entry state");
        assert!(
            state.typed.is_some(),
            "stale typed bundle should run EntryOnly gate prepare"
        );
        assert!(
            !state.resolution.by_symbol().is_empty(),
            "fast resolution path should still populate registry"
        );

        bump_typed_prepare_revision(&mut db, &entry_key);
        assert!(!is_typed_bundle_stale(&db, &entry_key));

        let resolution_only =
            entry_resolution_with_db(&mut db, &resolved, &options).expect("entry resolution");
        assert!(
            !resolution_only.by_symbol().is_empty(),
            "entry_resolution_with_db remains the fast path export"
        );

        typed_entry_state_with_db(&mut db, &resolved, &options, None).expect("typed entry state")
    };
    std::env::set_current_dir(previous).expect("restore cwd");

    let state = result;
    assert!(
        state.typed.is_some(),
        "caught-up typed prepare revision should produce executable bundle"
    );
    assert!(
        !state.resolution.by_symbol().is_empty(),
        "typed state should retain resolution"
    );
}

#[test]
fn manifest_digest_changes_when_manifest_or_lock_changes() {
    use beskid_queries::manifest_digest;

    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = dir.path().join("App.bproj");
    std::fs::write(&manifest, "manifest v1").expect("manifest");
    let digest_v1 = manifest_digest(&manifest);

    std::fs::write(&manifest, "manifest v2").expect("manifest update");
    let digest_v2 = manifest_digest(&manifest);
    assert_ne!(digest_v1, digest_v2);

    let lock = dir.path().join("Project.lock");
    std::fs::write(&lock, "lock v1").expect("lock");
    let digest_with_lock = manifest_digest(&manifest);
    assert_ne!(digest_v2, digest_with_lock);
}

fn test_session(db: &BeskidDatabase, entry_path: PathBuf) -> ProjectSession {
    ProjectSession::new(
        db,
        PathBuf::from("/tmp/project"),
        entry_path,
        "App".to_string(),
        "lock".to_string(),
    )
}

fn register_test_session(db: &mut BeskidDatabase, session: ProjectSession, entry_path: &Path) {
    db.project_registry()
        .lock()
        .expect("project registry")
        .insert(
            (
                PathBuf::from("/tmp/project"),
                entry_path
                    .canonicalize()
                    .unwrap_or_else(|_| entry_path.to_path_buf()),
                "App".to_string(),
            ),
            session,
        );
}

const MODULE_INDEX_STUB: &str = "mi:test:0:0";

#[test]
fn unit_type_surface_populates_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = PathBuf::from("/tmp/beskid_salsa_surface_populate.bd");
    let source = "i32 Main() { return 0; }".to_string();
    let fp = unit_content_fingerprint(&path, &source);
    db.ensure_file_text(path.clone(), source);

    let session = test_session(&db, path.clone());
    let _ = unit_type_surface(&db, session, path.clone(), MODULE_INDEX_STUB);
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .contains_key(&fp)
    );
}

#[test]
fn warm_second_type_surface_reuses_unit_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = PathBuf::from("/tmp/beskid_salsa_surface_warm.bd");
    let source = "i32 Main() { return 0; }".to_string();
    let fp = unit_content_fingerprint(&path, &source);
    db.ensure_file_text(path.clone(), source);

    let session = test_session(&db, path.clone());
    let _ = unit_type_surface(&db, session, path.clone(), MODULE_INDEX_STUB);
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .contains_key(&fp)
    );

    reset();
    let _ = unit_type_surface(&db, session, path.clone(), MODULE_INDEX_STUB);
    let (hits, misses, _) = snapshot();
    assert!(hits >= 1, "warm type surface should hit unit cache");
    assert!(
        misses <= 1,
        "warm type surface should not recompute cached surface (misses={misses})"
    );
}

#[test]
fn file_edit_invalidates_type_surface_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = PathBuf::from("/tmp/beskid_salsa_surface_edit.bd");
    db.ensure_file_text(path.clone(), "i32 Main() { return 0; }".to_string());

    let session = test_session(&db, path.clone());
    let _ = unit_type_surface(&db, session, path.clone(), MODULE_INDEX_STUB);
    assert!(
        !db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .is_empty()
    );

    db.set_file_text(path.clone(), "i32 Main() { return 42; }".to_string());
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .is_empty()
    );

    let fp_after = unit_content_fingerprint(&path, "i32 Main() { return 42; }");
    let _ = unit_type_surface(&db, session, path.clone(), MODULE_INDEX_STUB);
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .contains_key(&fp_after),
        "edited source should rebuild and cache a fresh type surface"
    );
}

#[test]
fn import_edit_invalidates_dependent_type_surface_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let dep_path = PathBuf::from("/tmp/beskid_salsa_test_dep.bd");
    let main_path = PathBuf::from("/tmp/beskid_salsa_test_consumer.bd");
    db.ensure_file_text(
        dep_path.clone(),
        "pub i32 Value() { return 1; }".to_string(),
    );
    db.ensure_file_text(main_path.clone(), "i32 Main() { return 0; }".to_string());

    let session = test_session(&db, main_path.clone());
    register_test_session(&mut db, session, &main_path);
    let dep_fp = unit_content_fingerprint(&dep_path, "pub i32 Value() { return 1; }");
    let main_fp = unit_content_fingerprint(&main_path, "i32 Main() { return 0; }");

    let _ = unit_type_surface(&db, session, dep_path.clone(), MODULE_INDEX_STUB);
    let _ = unit_type_surface(&db, session, main_path.clone(), MODULE_INDEX_STUB);
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .contains_key(&dep_fp)
    );
    assert!(
        db.unit_cache()
            .lock()
            .expect("unit cache")
            .unit_type_surfaces
            .contains_key(&main_fp)
    );

    db.invalidate_import_dependents(
        session,
        dep_path.clone(),
        vec![main_path.clone(), dep_path.clone()],
    );
    let cache = db.unit_cache().lock().expect("unit cache");
    assert!(!cache.unit_type_surfaces.contains_key(&dep_fp));
    assert!(
        cache.unit_type_surfaces.contains_key(&main_fp),
        "units without import edges should keep cached surfaces"
    );
}
