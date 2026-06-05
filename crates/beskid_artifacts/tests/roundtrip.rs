use beskid_artifacts::{
    AstUnitSnapshot, UnitArtifactMeta, ARTIFACT_SCHEMA_VERSION, content_fingerprint, encode_ast,
    decode_ast, grammar_revision, ArtifactStore,
};

#[test]
fn content_fingerprint_is_path_independent() {
    let fp1 = content_fingerprint("i32 main() { return 0; }");
    let fp2 = content_fingerprint("i32 main() { return 0; }");
    assert_eq!(fp1, fp2);
    assert_ne!(fp1, content_fingerprint("i32 main() { return 1; }"));
}

#[test]
fn ast_snapshot_roundtrips_postcard() {
    let meta = UnitArtifactMeta {
        content_fingerprint: content_fingerprint("source"),
        schema_version: ARTIFACT_SCHEMA_VERSION,
        grammar_rev: grammar_revision().to_string(),
        logical_name: "Main.bd".to_string(),
        source_path: std::path::PathBuf::from("/tmp/Main.bd"),
        source_len: 6,
        imports: vec!["std.io".to_string()],
    };
    let snapshot = AstUnitSnapshot::new(meta, vec![1, 2, 3, 4]);
    let bytes = encode_ast(&snapshot).expect("encode");
    let decoded = decode_ast(&bytes).expect("decode");
    assert_eq!(decoded, snapshot);
}

#[test]
fn artifact_store_writes_and_reads() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(dir.path());
    store.ensure_dirs().expect("ensure");
    let fp = content_fingerprint("unit source");
    let ast = AstUnitSnapshot::new(
        UnitArtifactMeta {
            content_fingerprint: fp.clone(),
            schema_version: ARTIFACT_SCHEMA_VERSION,
            grammar_rev: grammar_revision().to_string(),
            logical_name: "U.bd".into(),
            source_path: dir.path().join("U.bd"),
            source_len: 11,
            imports: vec![],
        },
        vec![9, 8, 7],
    );
    let hir = beskid_artifacts::HirUnitSnapshot::new(fp.clone(), vec![5, 4, 3]);
    store.write_unit(&ast, &hir).expect("write");
    assert!(store.read_ast(&fp).is_some());
    assert!(store.read_hir(&fp).is_some());
}
