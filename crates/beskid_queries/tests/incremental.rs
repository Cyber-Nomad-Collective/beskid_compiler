//! Incremental invalidation tests for Salsa-backed unit queries.

use std::path::PathBuf;

use beskid_analysis::services::{SemanticSnapshot, SessionFingerprint, cached_semantic_snapshot};
use beskid_analysis::services::{
    get_or_insert_assembly, invalidate_entry_sessions, update_semantic_snapshot,
};
use beskid_queries::{
    BeskidDatabase, Db, ProjectSession, fingerprint_key, parse_and_expand_unit, record_query_hit,
    reset, semantic_snapshot, snapshot, unit_content_fingerprint, unit_hir, unit_imports,
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
    let result = (|| {
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
    })();
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
