use beskid_queries::{
    BeskidDatabase, bump_file_revision, configure_db_for_project, file_revision_for, reset_compilation_database,
    reset_process_compilation_database,
};

#[test]
fn file_revision_safe_after_reset_compilation_database() {
    let mut db = BeskidDatabase::default();
    bump_file_revision(&mut db, "test-entry-key");
    reset_compilation_database(&mut db);
    bump_file_revision(&mut db, "test-entry-key");
    assert_eq!(file_revision_for(&db, "test-entry-key"), 1);
}

/// Several databases live side by side (one per project in the language server). Typed-entry
/// revision inputs belong to the storage that created them, so one database never reads or
/// bumps another's inputs, and a clone (a parallel query handle) shares them.
#[test]
fn typed_entry_revisions_belong_to_their_database() {
    let mut first = BeskidDatabase::default();
    let mut second = BeskidDatabase::default();
    bump_file_revision(&mut first, "shared-entry-key");
    bump_file_revision(&mut first, "shared-entry-key");
    assert_eq!(file_revision_for(&second, "shared-entry-key"), 0);
    bump_file_revision(&mut second, "shared-entry-key");
    assert_eq!(file_revision_for(&first, "shared-entry-key"), 2);
    assert_eq!(file_revision_for(&second, "shared-entry-key"), 1);

    let handle = first.clone();
    assert_eq!(file_revision_for(&handle, "shared-entry-key"), 2);
    drop(handle);

    drop(second);
    bump_file_revision(&mut first, "shared-entry-key");
    assert_eq!(file_revision_for(&first, "shared-entry-key"), 3);
}

#[test]
fn configure_db_for_project_thread_local_is_safe() {
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

#[test]
fn process_reset_clears_thread_local_inputs_and_project_configuration() {
    let root = tempfile::tempdir().expect("tempdir");
    configure_db_for_project(root.path());
    beskid_queries::with_db(|db| bump_file_revision(db, "test-entry-key"));

    reset_process_compilation_database();
    configure_db_for_project(root.path());

    beskid_queries::with_db(|db| {
        bump_file_revision(db, "test-entry-key");
        assert_eq!(file_revision_for(db, "test-entry-key"), 1);
    });
}
