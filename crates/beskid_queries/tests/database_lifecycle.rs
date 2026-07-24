use std::path::Path;
use std::sync::Mutex;

use beskid_queries::{
    BeskidDatabase, bump_file_revision, configure_compilation_database_for_project, configure_db_for_project,
    file_revision_for, reset_compilation_database, reset_typed_entry_inputs,
};

static REGISTRY_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn file_revision_safe_after_reset_compilation_database() {
    let _guard = REGISTRY_TEST_LOCK.lock().expect("registry test lock");
    reset_typed_entry_inputs();
    let mut db = BeskidDatabase::default();
    bump_file_revision(&mut db, "test-entry-key");
    reset_compilation_database(&mut db);
    bump_file_revision(&mut db, "test-entry-key");
    assert_eq!(file_revision_for(&db, "test-entry-key"), 1);
}

#[test]
fn configure_compilation_database_clears_stale_inputs() {
    let _guard = REGISTRY_TEST_LOCK.lock().expect("registry test lock");
    reset_typed_entry_inputs();
    let mut db = BeskidDatabase::default();
    bump_file_revision(&mut db, "test-entry-key");
    configure_compilation_database_for_project(&mut db, Path::new("/tmp/beskid-db-lifecycle-test"));
    bump_file_revision(&mut db, "test-entry-key");
    assert_eq!(file_revision_for(&db, "test-entry-key"), 1);
}

#[test]
fn configure_db_for_project_thread_local_is_safe() {
    let _guard = REGISTRY_TEST_LOCK.lock().expect("registry test lock");
    reset_typed_entry_inputs();
    let mut db = BeskidDatabase::default();
    bump_file_revision(&mut db, "test-entry-key");
    let first = tempfile::tempdir().expect("tempdir");
    let second = tempfile::tempdir().expect("tempdir");
    configure_db_for_project(first.path());
    configure_db_for_project(second.path());
    beskid_queries::with_db(|db| {
        bump_file_revision(db, "test-entry-key");
        assert_eq!(file_revision_for(db, "test-entry-key"), 1);
    });
}
