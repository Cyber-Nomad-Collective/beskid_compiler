use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::{
    CANONICAL_NETWORK_INTERNAL_SOURCE_PATH, canonical_corelib_service_capability, canonical_corelib_service_source_path,
};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, aggregate_field_access,
    aggregate_literal_declaration, aggregate_literal_layout, build_typed_program_with_corelib_services,
};
use std::{path::PathBuf, sync::Arc};

fn fact_result<T>(
    owner_path: PathBuf,
    owner_source: String,
    deadline_path: PathBuf,
    deadline_source: String,
    marker: &str,
    kind: NodeKind,
    query: impl FnOnce(&BeskidDatabase, AstNodeKey) -> Result<Option<T>, beskid_queries::SemanticError>,
) -> Result<Option<T>, beskid_queries::SemanticError> {
    let mut db = BeskidDatabase::default();
    let generation = SyntaxGenerationId(121);
    let owner_program = parse_program(&owner_source).expect("parse projection owner");
    let deadline_program = parse_program(&deadline_source).expect("parse Deadline declaration");
    let index = SyntaxIndex::from_program(&owner_program, generation);
    let marker = owner_source.find(marker).expect("fact fixture");
    let node = index
        .metadata()
        .iter()
        .find(|metadata| metadata.kind == kind && metadata.span.is_some_and(|span| span.start == marker))
        .expect("fact node")
        .id;
    let owner_root = owner_path.parent().expect("owner root").to_path_buf();
    let foundation_root = deadline_path.ancestors().nth(3).expect("foundation source root").to_path_buf();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: owner_root.clone() },
            dependencies: vec![RootEntry { dependency_name: Some("foundation".into()), source_root: foundation_root }],
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: "Core/Time/Deadline.bd".into(),
                origin_path: deadline_path.clone(),
                path: deadline_path,
                source: deadline_source,
                program: deadline_program,
            },
            SourceUnit {
                logical_name: owner_path.display().to_string(),
                origin_path: owner_path.clone(),
                path: owner_path.clone(),
                source: owner_source,
                program: owner_program,
            },
        ]),
        1,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let project = ProjectSession::new(&db, owner_root, owner_path.clone(), "deadline-projection".into(), "lock".into());
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&manifest).expect("corelib capability"),
    )
    .expect("typed program");
    query(&db, AstNodeKey { unit: SourceUnitId::new(&db, owner_path), generation, node })
}

fn projection_result(
    owner_path: PathBuf,
    owner_source: String,
    deadline_path: PathBuf,
    deadline_source: String,
) -> Result<Option<beskid_queries::AggregateFieldAccess>, beskid_queries::SemanticError> {
    fact_result(
        owner_path,
        owner_source,
        deadline_path,
        deadline_source,
        "deadline.monotonicNanos",
        NodeKind::PathExpression,
        |db, key| aggregate_field_access(db, key),
    )
}

fn canonical_deadline() -> (PathBuf, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/foundation/src/Core/Time/Deadline.bd")
        .canonicalize()
        .expect("canonical Deadline source");
    let source = std::fs::read_to_string(&path).expect("read canonical Deadline source");
    (path, source)
}

#[test]
fn deadline_projection_rejects_ordinary_source() {
    let temp = tempfile::tempdir().expect("application root");
    let owner_path = temp.path().join("Main.bd");
    let owner_source = "use Core.Time.Deadline; i64 Reveal(Deadline deadline) { return deadline.monotonicNanos; }";
    let (deadline_path, deadline_source) = canonical_deadline();
    assert!(
        projection_result(owner_path, owner_source.into(), deadline_path, deadline_source).is_err(),
        "an application cannot project Deadline's private monotonic sample"
    );
}

#[test]
fn deadline_projection_rejects_a_lookalike_nominal_type() {
    let temp = tempfile::tempdir().expect("application root");
    let owner_path = temp.path().join("Main.bd");
    let deadline_path = temp.path().join("Core/Time/Deadline.bd");
    let owner_source = "use Core.Time.Deadline; i64 Reveal(Deadline deadline) { return deadline.monotonicNanos; }";
    let deadline_source = "pub type Deadline { i64 monotonicNanos }";
    assert!(
        projection_result(owner_path, owner_source.into(), deadline_path, deadline_source.into()).is_err(),
        "a same-named user type cannot acquire canonical Deadline projection authority"
    );
}

#[test]
fn deadline_projection_admits_only_canonical_network_internal() {
    let owner_path = canonical_corelib_service_source_path(CANONICAL_NETWORK_INTERNAL_SOURCE_PATH)
        .expect("canonical Network/Internal source");
    let owner_source = std::fs::read_to_string(&owner_path).expect("read canonical Network/Internal source");
    let (deadline_path, deadline_source) = canonical_deadline();
    assert!(
        projection_result(owner_path, owner_source, deadline_path, deadline_source)
            .expect("canonical Network/Internal projection")
            .is_some(),
        "only the canonical Network/Internal source may project Deadline"
    );
}

#[test]
fn deadline_projection_rejects_a_copied_network_internal_source() {
    let temp = tempfile::tempdir().expect("copied Network root");
    let owner_path = temp.path().join("Network/Internal.bd");
    let canonical_path = canonical_corelib_service_source_path(CANONICAL_NETWORK_INTERNAL_SOURCE_PATH)
        .expect("canonical Network/Internal source");
    let owner_source = std::fs::read_to_string(canonical_path).expect("read canonical Network/Internal source");
    let (deadline_path, deadline_source) = canonical_deadline();
    assert!(
        projection_result(owner_path, owner_source, deadline_path, deadline_source).is_err(),
        "copied canonical bytes outside the trusted Corelib root grant no projection authority"
    );
}

#[test]
fn deadline_projection_rejects_lookalike_even_inside_canonical_network_internal() {
    let temp = tempfile::tempdir().expect("lookalike Deadline root");
    let owner_path = canonical_corelib_service_source_path(CANONICAL_NETWORK_INTERNAL_SOURCE_PATH)
        .expect("canonical Network/Internal source");
    let owner_source = std::fs::read_to_string(&owner_path).expect("read canonical Network/Internal source");
    let deadline_path = temp.path().join("Core/Time/Deadline.bd");
    let deadline_source = "pub type Deadline { i64 monotonicNanos }";
    assert!(
        projection_result(owner_path, owner_source, deadline_path, deadline_source.into()).is_err(),
        "a canonical owner cannot project a same-named user type"
    );
}

#[test]
fn deadline_literal_rejects_raw_construction_in_ordinary_source() {
    let temp = tempfile::tempdir().expect("application root");
    let owner_path = temp.path().join("Main.bd");
    let owner_source = "use Core.Time.Deadline; Deadline Forge() { return Deadline { monotonicNanos: 1_i64 }; }";
    let (deadline_path, deadline_source) = canonical_deadline();
    assert!(
        fact_result(
            owner_path.clone(),
            owner_source.into(),
            deadline_path.clone(),
            deadline_source.clone(),
            "Deadline { monotonicNanos",
            NodeKind::StructLiteralExpression,
            |db, key| aggregate_literal_layout(db, key),
        )
        .is_err(),
        "ordinary source cannot construct opaque Deadline from raw ticks"
    );
    assert!(
        fact_result(
            owner_path,
            owner_source.into(),
            deadline_path,
            deadline_source,
            "Deadline { monotonicNanos",
            NodeKind::StructLiteralExpression,
            |db, key| aggregate_literal_declaration(db, key),
        )
        .is_err(),
        "ordinary source cannot resolve a private-field Deadline constructor"
    );
}

#[test]
fn deadline_literal_does_not_restrict_an_unrelated_lookalike() {
    let temp = tempfile::tempdir().expect("lookalike type root");
    let owner_path = temp.path().join("Main.bd");
    let deadline_path = temp.path().join("Core/Time/Deadline.bd");
    let owner_source = "use Core.Time.Deadline; Deadline Make() { return Deadline { monotonicNanos: 1_i64 }; }";
    let deadline_source = "pub type Deadline { i64 monotonicNanos }";
    assert!(
        fact_result(
            owner_path,
            owner_source.into(),
            deadline_path,
            deadline_source.into(),
            "Deadline { monotonicNanos",
            NodeKind::StructLiteralExpression,
            |db, key| aggregate_literal_layout(db, key),
        )
        .expect("unrelated lookalike remains constructible")
        .is_some()
    );
}

#[test]
fn deadline_literal_allows_constructor_inside_canonical_declaring_unit() {
    let (path, source) = canonical_deadline();
    let program = parse_program(&source).expect("parse canonical Deadline source");
    let generation = SyntaxGenerationId(122);
    let index = SyntaxIndex::from_program(&program, generation);
    let marker = source.find("Deadline { monotonicNanos").expect("canonical Deadline constructor");
    let node = index
        .metadata()
        .iter()
        .find(|metadata| {
            metadata.kind == NodeKind::StructLiteralExpression && metadata.span.is_some_and(|span| span.start == marker)
        })
        .expect("canonical Deadline literal")
        .id;
    let mut db = BeskidDatabase::default();
    let root = path.ancestors().nth(3).expect("foundation source root").to_path_buf();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Core/Time/Deadline.bd".into(),
            origin_path: path.clone(),
            path: path.clone(),
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let project = ProjectSession::new(&db, root, path.clone(), "foundation".into(), "lock".into());
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        assembly,
        canonical_corelib_service_capability(&AbiManifestV5::canonical_runtime(target)).expect("corelib capability"),
    )
    .expect("typed canonical Deadline source");
    assert!(
        aggregate_literal_layout(&db, AstNodeKey { unit: SourceUnitId::new(&db, path), generation, node })
            .expect("canonical constructor layout")
            .is_some()
    );
}
