use beskid_artifacts::{
    ARTIFACT_SCHEMA_VERSION, ArtifactManifest, ArtifactStore, AstUnitSnapshot, UnitArtifactMeta, content_fingerprint,
    decode_ast, encode_ast, grammar_revision,
};

#[test]
fn content_fingerprint_is_path_independent() {
    let fp1 = content_fingerprint("i32 Main() { return 0; }");
    let fp2 = content_fingerprint("i32 Main() { return 0; }");
    assert_eq!(fp1, fp2);
    assert_ne!(fp1, content_fingerprint("i32 Main() { return 1; }"));
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
    store.write_unit(&ast).expect("write");
    assert!(store.read_ast(&fp).is_some());
    let unit_dir = store.unit_paths(&fp).unit_dir;
    let files = std::fs::read_dir(unit_dir)
        .expect("unit directory")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 2, "only syntax and metadata are persisted");
    assert!(files.contains(&"ast.bin".into()));
    assert!(files.contains(&"meta.json".into()));
}

#[test]
fn writing_schema_v2_removes_legacy_hir_payload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(dir.path());
    let fp = content_fingerprint("migrated unit");
    let paths = store.unit_paths(&fp);
    std::fs::create_dir_all(&paths.unit_dir).expect("legacy unit directory");
    std::fs::write(paths.unit_dir.join("hir.bin"), b"legacy hir").expect("legacy payload");
    let ast = AstUnitSnapshot::new(
        UnitArtifactMeta {
            content_fingerprint: fp,
            schema_version: ARTIFACT_SCHEMA_VERSION,
            grammar_rev: grammar_revision().to_string(),
            logical_name: "Migrated.bd".into(),
            source_path: dir.path().join("Migrated.bd"),
            source_len: 13,
            imports: vec![],
        },
        vec![1, 2, 3],
    );

    store.write_unit(&ast).expect("schema-v2 write");

    assert!(!paths.unit_dir.join("hir.bin").exists());
    assert!(paths.ast.exists());
    assert!(paths.meta.exists());
}

#[test]
fn schema_migration_purges_all_legacy_units_before_rewriting_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(dir.path());
    let units = store.cache_root().join("units");
    for fingerprint in ["rewritten", "untouched"] {
        let unit = units.join(fingerprint);
        std::fs::create_dir_all(&unit).expect("legacy unit directory");
        std::fs::write(unit.join("hir.bin"), b"legacy hir").expect("legacy HIR");
        std::fs::write(unit.join("ast.bin"), b"legacy AST").expect("legacy AST");
    }
    std::fs::create_dir_all(store.cache_root()).expect("cache root");
    let legacy_manifest = ArtifactManifest {
        grammar_rev: grammar_revision().to_string(),
        compiler_version: "legacy".to_string(),
        schema_version: ARTIFACT_SCHEMA_VERSION - 1,
        persisted_units: 2,
    };
    std::fs::write(
        store.cache_root().join("manifest.json"),
        serde_json::to_string(&legacy_manifest).expect("legacy manifest"),
    )
    .expect("write legacy manifest");

    let current = AstUnitSnapshot::new(
        UnitArtifactMeta {
            content_fingerprint: "rewritten".to_string(),
            schema_version: ARTIFACT_SCHEMA_VERSION,
            grammar_rev: grammar_revision().to_string(),
            logical_name: "Current.bd".into(),
            source_path: dir.path().join("Current.bd"),
            source_len: 7,
            imports: vec![],
        },
        vec![4, 2],
    );
    store.write_unit(&current).expect("migrate and rewrite one unit");

    assert!(store.unit_paths("rewritten").ast.exists());
    assert!(!store.unit_paths("untouched").unit_dir.exists());
    fn count_hir_files(path: &std::path::Path) -> usize {
        std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| {
                if entry.path().is_dir() {
                    count_hir_files(&entry.path())
                } else {
                    usize::from(entry.file_name() == "hir.bin")
                }
            })
            .sum()
    }
    let hir_files = count_hir_files(store.cache_root());
    assert_eq!(hir_files, 0, "migration must purge every legacy HIR payload");
    assert_eq!(store.manifest().expect("current manifest").schema_version, ARTIFACT_SCHEMA_VERSION);
}

#[test]
fn store_written_under_another_grammar_revision_is_invalidated_silently() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(dir.path());
    let fp = content_fingerprint("unit source");
    let unit = |grammar_rev: &str| {
        AstUnitSnapshot::new(
            UnitArtifactMeta {
                content_fingerprint: fp.clone(),
                schema_version: ARTIFACT_SCHEMA_VERSION,
                grammar_rev: grammar_rev.to_string(),
                logical_name: "U.bd".into(),
                source_path: dir.path().join("U.bd"),
                source_len: 11,
                imports: vec![],
            },
            vec![9, 8, 7],
        )
    };
    let stale_revision = format!("{}-older-grammar", grammar_revision());

    // A unit whose own metadata names another grammar revision is never read back.
    store.write_unit(&unit(&stale_revision)).expect("write stale unit");
    assert!(store.read_ast(&fp).is_none(), "a unit tree from another grammar revision must not be adopted");

    // A manifest written by another grammar revision makes the whole store stale: reads miss and
    // the next write replaces the unit namespace instead of failing.
    store.write_unit(&unit(grammar_revision())).expect("write current unit");
    assert!(store.read_ast(&fp).is_some());
    let manifest_path = store.cache_root().join("manifest.json");
    let mut manifest: ArtifactManifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).expect("manifest")).expect("manifest json");
    manifest.grammar_rev = stale_revision;
    std::fs::write(&manifest_path, serde_json::to_string(&manifest).expect("encode")).expect("rewrite manifest");
    assert!(!store.is_manifest_current());
    assert!(store.read_ast(&fp).is_none(), "a stale manifest invalidates every persisted tree");
    store.ensure_dirs().expect("stale store is rebuilt, not an error");
    assert!(store.is_manifest_current());
    assert!(store.read_ast(&fp).is_none(), "stale units are purged with the old manifest");
}
