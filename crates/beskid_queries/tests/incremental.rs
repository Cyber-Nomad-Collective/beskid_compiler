//! Incremental invalidation tests for Salsa-backed unit queries.

use std::path::PathBuf;

use beskid_queries::{
    parse_and_expand_unit, reset, snapshot, unit_content_fingerprint, unit_hir, unit_imports,
    BeskidDatabase, ProjectSession,
};

fn fixture_source() -> String {
    "use std.io;\ni32 main() { return 0; }".to_string()
}

fn fixture_path() -> PathBuf {
    PathBuf::from("/tmp/beskid_salsa_test_main.bd")
}

#[test]
fn unit_fingerprint_changes_when_source_changes() {
    let path = fixture_path();
    let fp1 = unit_content_fingerprint(&path, "i32 main() { return 0; }");
    let fp2 = unit_content_fingerprint(&path, "i32 main() { return 1; }");
    assert_ne!(fp1, fp2);
}

#[test]
fn second_parse_hits_unit_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = fixture_path();
    let source = fixture_source();
    db.ensure_file_text(path.clone(), source);

    let session = ProjectSession::new(
        &mut db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );

    let _ = parse_and_expand_unit(&db, session, path.clone());
    let (hits_before, _, _) = snapshot();
    let _ = parse_and_expand_unit(&db, session, path.clone());
    let (hits_after, _, _) = snapshot();
    assert!(hits_after > hits_before);
}

#[test]
fn file_edit_invalidates_unit_cache() {
    reset();
    let mut db = BeskidDatabase::default();
    let path = fixture_path();
    db.ensure_file_text(path.clone(), fixture_source());

    let session = ProjectSession::new(
        &mut db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );

    let _ = unit_hir(&db, session, path.clone());
    reset();
    db.set_file_text(path.clone(), "i32 main() { return 99; }".to_string());
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
        &mut db,
        PathBuf::from("/tmp/project"),
        path.clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let imports = unit_imports(&db, session, path);
    assert!(imports.iter().any(|import| import.contains("std.io")));
}

#[test]
fn manifest_digest_changes_when_manifest_or_lock_changes() {
    use beskid_queries::manifest_digest;

    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = dir.path().join("Project.proj");
    std::fs::write(&manifest, "project v1").expect("manifest");
    let digest_v1 = manifest_digest(&manifest);

    std::fs::write(&manifest, "project v2").expect("manifest update");
    let digest_v2 = manifest_digest(&manifest);
    assert_ne!(digest_v1, digest_v2);

    let lock = dir.path().join("Project.lock");
    std::fs::write(&lock, "lock v1").expect("lock");
    let digest_with_lock = manifest_digest(&manifest);
    assert_ne!(digest_v2, digest_with_lock);
}
