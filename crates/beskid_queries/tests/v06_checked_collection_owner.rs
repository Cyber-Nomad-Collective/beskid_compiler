//! A nested receiver is distinct from its rooted lexical ancestor.
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::canonical_corelib_service_capability;
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ResolvedDependencyProject, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, CollectionMutationOwner, CollectionOperation, SourceUnitId, aggregate_field_access,
    build_typed_program_with_corelib_services, collection_operation, project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

fn fixture(
    copied_source: Option<&str>,
) -> (tempfile::TempDir, BeskidDatabase, Arc<beskid_analysis::projects::ProgramAssembly>, AstNodeKey) {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().canonicalize().unwrap().join("Src");
    std::fs::create_dir_all(&source_root).unwrap();
    if let Some(source) = copied_source {
        std::fs::create_dir_all(source_root.join("Core/Collections/Array")).unwrap();
        std::fs::write(source_root.join("Core/Collections/Array.bd"), source).unwrap();
        std::fs::write(
            source_root.join("Core/Collections/Array/ArrayIter.bd"),
            include_str!("../../../corelib/packages/foundation/src/Core/Collections/Array/ArrayIter.bd"),
        )
        .unwrap();
    }
    let foundation = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/foundation")
        .canonicalize()
        .unwrap();
    let path = source_root.join("Main.bd");
    std::fs::write(&path, "use Core.Collections.Array;\npub type Profile { pub i64[] roots, } pub type Loader { pub Profile profile, } pub unit Main(Loader loader) { Array.TryAppend<i64>(loader.profile.roots, 7_i64); return; }").unwrap();
    let plan = CompilePlan {
        project_root: source_root.parent().unwrap().to_owned(),
        manifest_path: source_root.parent().unwrap().join("Host.bproj"),
        project_name: "Host".into(),
        source_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: if copied_source.is_none() {
            vec![ResolvedDependencyProject {
                dependency_name: "corelib_foundation".into(),
                manifest_path: foundation.join("corelib_foundation.bproj"),
                project_root: foundation.clone(),
                project_name: "corelib_foundation".into(),
                source_root: foundation.join("src"),
            }]
        } else {
            vec![]
        },
        unresolved_dependencies: vec![],
        has_std_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "owner-test".into()).unwrap();
    let manifest = AbiManifestV5::canonical_runtime(
        TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .unwrap(),
    );
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        assembly.generation,
        assembly.clone(),
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let node = assembly.entry_syntax_index().ids_of_kind(NodeKind::CallExpression).next().unwrap();
    let call = AstNodeKey { unit, generation: assembly.generation, node };
    (root, db, assembly, call)
}

#[test]
fn checked_append_retains_exact_nested_mutable_owner() {
    let (_root, db, assembly, call) = fixture(None);
    let Some(CollectionOperation::TryAppend { owner: CollectionMutationOwner::AggregateField { receiver, root, .. } }) =
        collection_operation(&db, call).unwrap()
    else {
        panic!("checked owner proof missing");
    };
    let projection = aggregate_field_access(&db, receiver).unwrap().unwrap();
    assert_ne!(receiver, projection.receiver);
    assert_eq!(beskid_queries::local_slot(&db, projection.receiver).unwrap(), Some(root));
    let stale = AstNodeKey { generation: beskid_queries::SyntaxGenerationId(assembly.generation.0 + 1), ..call };
    assert!(collection_operation(&db, stale).unwrap().is_none());
}

#[test]
fn copied_checked_array_source_cannot_issue_owner_transaction() {
    let source = include_str!("../../../corelib/packages/foundation/src/Core/Collections/Array.bd");
    let (_root, db, _assembly, call) = fixture(Some(source));
    assert!(matches!(beskid_queries::call_lowering(&db, call).unwrap(), Some(beskid_queries::CallLowering::Direct(_))));
    assert!(collection_operation(&db, call).unwrap().is_none());
}
