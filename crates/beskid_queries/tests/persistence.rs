#![cfg(feature = "persistence")]

use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, IndexedNodeKind, ProjectSession, SourceUnitId, SyntaxGenerationId, node_kind,
    save_db_snapshot,
};

fn snapshot_fixture() -> (tempfile::TempDir, std::path::PathBuf, AstNodeKey) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.bd");
    let source = "i64 Main() { return 42_i64; }";
    std::fs::write(&path, source).unwrap();
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, path.clone());
    let project = ProjectSession::new(&db, directory.path().into(), path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(19);
    db.ensure_file_text(path.clone(), source.into());
    db.ensure_syntax_unit(project, unit, generation).unwrap();
    let program = beskid_analysis::services::parse_program(source).unwrap();
    let index = SyntaxIndex::from_program(&program, generation);
    let node = index.ids_of_kind(NodeKind::FunctionDefinition).next().unwrap();
    assert_eq!(
        node_kind(&db, AstNodeKey { unit, generation, node }).unwrap(),
        Some(IndexedNodeKind::FunctionDefinition)
    );
    let cache = beskid_queries::cache_root_for_project(directory.path());
    save_db_snapshot(&mut db, &cache).unwrap();
    (directory, path, AstNodeKey { unit, generation, node })
}

#[test]
fn ordered_snapshot_round_trips_real_syntax_query_memos() {
    let (directory, path, key) = snapshot_fixture();
    let restored = BeskidDatabase::with_persistence(directory.path());
    assert!(restored.file_text(&path).is_some(), "a complete snapshot must publish its input registries");
    let unit = SourceUnitId::new(&restored, path);
    assert!(restored.syntax_unit(unit).is_some(), "real syntax input survives reload");
    assert_eq!(node_kind(&restored, AstNodeKey { unit, ..key }).unwrap(), Some(IndexedNodeKind::FunctionDefinition));
}

#[test]
fn fresh_snapshot_revalidates_real_call_abi_before_call_lowering_is_requested() {
    use salsa::Database;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.bd");
    let source = "i64 Value() { return 3_i64; } i64 Main() { return Value(); }";
    std::fs::write(&path, source).unwrap();
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, path.clone());
    let project = ProjectSession::new(&db, directory.path().into(), path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(19);
    db.ensure_file_text(path.clone(), source.into());
    db.ensure_syntax_unit(project, unit, generation).unwrap();
    let program = beskid_analysis::services::parse_program(source).unwrap();
    let index = SyntaxIndex::from_program(&program, generation);
    let node = index.ids_of_kind(NodeKind::CallExpression).next().unwrap();
    let key = AstNodeKey { unit, generation, node };
    assert_eq!(beskid_queries::abi_type(&db, key).unwrap(), Some(beskid_queries::SemanticTypeId::I64));
    save_db_snapshot(&mut db, &beskid_queries::cache_root_for_project(directory.path())).unwrap();

    let mut restored = BeskidDatabase::with_persistence(directory.path());
    assert!(restored.file_text(&path).is_some());
    restored.synthetic_write(salsa::Durability::LOW);
    // ABI is the parent query. Its persisted call-lowering dependencies have not
    // been requested directly on this fresh compiler database.
    assert_eq!(beskid_queries::abi_type(&restored, key).unwrap(), Some(beskid_queries::SemanticTypeId::I64));
}

#[test]
fn warm_snapshot_source_edit_resumes_assembly_generation_in_a_fresh_process() {
    use beskid_analysis::services::{PrepareOptions, ResolvedInput, synthetic_compile_plan_for_source};

    let prepare = |db: &mut BeskidDatabase, path: &std::path::Path| {
        let resolved = ResolvedInput {
            source_path: path.into(),
            source: std::fs::read_to_string(path).unwrap(),
            compile_plan: Some(synthetic_compile_plan_for_source(path)),
            prepared_workspace: None,
            workspace_summary: None,
            assembly: None,
        };
        let (prepared, diagnostics, _) =
            beskid_queries::prepare_compilation_diagnostics_with_db(db, &resolved, PrepareOptions::default(), None)
                .expect("source edit must prepare against restored syntax authority");
        assert!(
            !diagnostics.iter().any(|diagnostic| diagnostic.severity == beskid_analysis::Severity::Error),
            "{diagnostics:?}"
        );
        assert!(prepared.typed.is_some(), "the regression must materialize real executable frontend facts");
        prepared
    };
    if let Ok(root) = std::env::var("BESKID_GENERATION_REPLAY_FIXTURE") {
        let root = std::path::PathBuf::from(root);
        let path = root.join("Main.bd");
        let previous: u64 = std::env::var("BESKID_GENERATION_REPLAY_PREVIOUS").unwrap().parse().unwrap();
        let mut restored = BeskidDatabase::with_persistence(&root);
        assert!(restored.file_text(&path).is_some(), "fixture must load, not bypass, its snapshot");
        let unit = SourceUnitId::new(&restored, path.clone());
        assert!(restored.syntax_unit(unit).unwrap().accepts_key(
            &restored,
            AstNodeKey { unit, generation: SyntaxGenerationId(previous), node: beskid_queries::AstNodeId(0) }
        ));
        let next = prepare(&mut restored, &path);
        assert!(next.assembly.generation.0 > previous, "restored generation must precede the edited assembly");
        let edited =
            std::fs::read_to_string(&path).unwrap().replace("return Result::Ok(2_i64)", "return Result::Ok(3_i64)");
        std::fs::write(&path, edited).unwrap();
        // Keep `next.typed` alive so this also exercises the executable cache's
        // weak handle, not just an entry containing an expired executable.
        let following = prepare(&mut restored, &path);
        assert!(
            following.assembly.generation > next.assembly.generation,
            "live edits must keep advancing after restoration"
        );
        assert!(following.assembly.entry_unit().source.contains("return Result::Ok(3_i64)"));
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.bd");
    let source = "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } enum Problem { Closed() } Result<i64, Problem> Accept() { return Result::Ok(1_i64); } Result<unit, Problem> Main() { i64 value = Accept()?; return Result::Ok(()); }";
    std::fs::write(&path, source).unwrap();
    let mut db = BeskidDatabase::default();
    let previous = prepare(&mut db, &path).assembly.generation;
    save_db_snapshot(&mut db, &beskid_queries::cache_root_for_project(directory.path())).unwrap();
    std::fs::write(&path, source.replace("return Result::Ok(1_i64)", "return Result::Ok(2_i64)")).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "warm_snapshot_source_edit_resumes_assembly_generation_in_a_fresh_process", "--nocapture"])
        .env("BESKID_GENERATION_REPLAY_FIXTURE", directory.path())
        .env("BESKID_GENERATION_REPLAY_PREVIOUS", previous.0.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fresh-process replay failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope {
    version: String,
    compiler: String,
    digest: String,
    db: Box<serde_json::value::RawValue>,
}

fn rewrite_snapshot(directory: &std::path::Path, edit: impl FnOnce(&mut Envelope)) {
    let path = beskid_queries::cache_root_for_project(directory).join("db.json");
    let mut envelope: Envelope = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    edit(&mut envelope);
    std::fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
}

fn replace_payload(envelope: &mut Envelope, payload: String) {
    use sha2::Digest;
    envelope.digest = format!("{:x}", sha2::Sha256::digest(payload.as_bytes()));
    envelope.db = serde_json::value::RawValue::from_string(payload).unwrap();
}

fn assert_rejected_and_usable(directory: &std::path::Path, rejected_file: &std::path::Path) {
    let mut db = BeskidDatabase::default();
    let sentinel = directory.join("Sentinel.bd");
    db.ensure_file_text(sentinel.clone(), "use Preserved;".into());
    assert!(!beskid_queries::load_db_snapshot(&mut db, &beskid_queries::cache_root_for_project(directory)));
    assert!(db.file_text(rejected_file).is_none(), "candidate registries must not be published");
    assert_eq!(db.file_text(&sentinel).unwrap().text(&db), "use Preserved;");
    let project = ProjectSession::new(&db, directory.into(), sentinel.clone(), "Sentinel".into(), "lock".into());
    let grammar = db.grammar_revision();
    assert_eq!(beskid_queries::unit_imports(&db, project, grammar, sentinel), vec!["Preserved"]);
}

#[test]
fn malformed_memo_rejection_leaves_the_live_database_usable() {
    let (directory, path, _) = snapshot_fixture();
    rewrite_snapshot(directory.path(), |envelope| {
        let original = envelope.db.get();
        let corrupt = original.replace("\"Ok\":\"FunctionDefinition\"", "\"Ok\":\"InvalidNodeKind\"");
        assert_ne!(corrupt, original, "corrupt the actual persisted node-kind memo");
        replace_payload(envelope, corrupt);
    });
    assert_rejected_and_usable(directory.path(), &path);
}

#[test]
fn out_of_order_snapshot_is_rejected_before_salsa_mutation() {
    let (directory, path, _) = snapshot_fixture();
    rewrite_snapshot(directory.path(), |envelope| {
        let payload: serde_json::Value = serde_json::from_str(envelope.db.get()).unwrap();
        let ingredients = payload["ingredients"].as_object().unwrap();
        let mut entries = ingredients.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(_, ingredient)| !ingredient.as_object().unwrap().keys().any(|key| key.contains(':')));
        let entries = entries
            .into_iter()
            .map(|(key, value)| format!("{}:{}", serde_json::to_string(key).unwrap(), value))
            .collect::<Vec<_>>()
            .join(",");
        replace_payload(envelope, format!("{{\"runtime\":{},\"ingredients\":{{{entries}}}}}", payload["runtime"]));
    });
    assert_rejected_and_usable(directory.path(), &path);
}

#[test]
fn incompatible_or_damaged_snapshot_preserves_the_live_database() {
    for component in ["version", "compiler", "digest"] {
        let (directory, path, _) = snapshot_fixture();
        rewrite_snapshot(directory.path(), |envelope| match component {
            "version" => envelope.version.push_str("-incompatible"),
            "compiler" => envelope.compiler = "other-schema-build".into(),
            "digest" => envelope.digest = "damaged".into(),
            _ => unreachable!(),
        });
        assert_rejected_and_usable(directory.path(), &path);
    }
}

#[test]
fn legacy_owned_value_snapshot_is_rejected_without_touching_the_database() {
    let (directory, path, _) = snapshot_fixture();
    let snapshot = beskid_queries::cache_root_for_project(directory.path()).join("db.json");
    let mut legacy: serde_json::Value = serde_json::from_slice(&std::fs::read(&snapshot).unwrap()).unwrap();
    let object = legacy.as_object_mut().unwrap();
    object.remove("compiler");
    object.remove("digest");
    object.insert("version".into(), "beskid-queries:0.1.0:grammar:0.1.0".into());
    std::fs::write(snapshot, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_rejected_and_usable(directory.path(), &path);
}
