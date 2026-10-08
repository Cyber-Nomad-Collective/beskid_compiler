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
    std::fs::write(&path, "use Core.Collections.Array;\npub type Profile { pub i64[] roots, } pub type Loader { pub Profile profile, } pub unit Main(Loader loader) { Array.Append<i64>(loader.profile.roots, 7_i64); return; }").unwrap();
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
        has_core_dependency: false,
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
    let program = AstNodeKey { node: assembly.entry_syntax_index().ids_of_kind(NodeKind::Program).next().unwrap(), ..call };
    let imports = beskid_queries::unresolved_imports(&db, program).unwrap().unwrap();
    for source in assembly.units.iter() {
        eprintln!("assembled unit {:?}, origin {:?}, module {:?}", source.path, source.origin_path,
            beskid_analysis::projects::infer_logical_module_path(source, &assembly.roots));
    }
    assert!(imports.is_empty(), "real materializer must register the Array import: {imports:?}");
    let parsed_call = assembly
        .entry_syntax_index()
        .node_at(&assembly.entry_unit().program, node)
        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
        .unwrap();
    eprintln!("owner call syntax {:?}; generation {:?}", parsed_call.callee, assembly.generation);
    for source in assembly.units.iter() {
        eprintln!("owner assembled unit {:?}, module {:?}, origin {:?}", source.path,
            beskid_analysis::projects::infer_logical_module_path(source, &assembly.roots), source.origin_path);
        let source_unit = SourceUnitId::new(&db, source.path.clone());
        for declaration in
            assembly.syntax_index_for_path(&source.path).unwrap().ids_of_kind(NodeKind::FunctionDefinition)
        {
            let key = AstNodeKey { unit: source_unit, generation: assembly.generation, node: declaration };
            if beskid_queries::item_name(&db, key).unwrap().as_deref() == Some("Append") {
                let syntax = assembly
                    .syntax_index_for_path(&source.path)
                    .unwrap()
                    .node_at(&source.program, declaration)
                    .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
                    .unwrap();
                eprintln!(
                    "registered Append {:?}, generics {:?}; instantiation {:?}",
                    source.path,
                    syntax.generics,
                    beskid_queries::generic_call_instantiation(&db, call)
                );
            }
        }
    }
    (root, db, assembly, call)
}

#[test]
fn nested_collection_owner_retains_terminal_receiver_and_rooted_ancestor() {
    let (_root, db, assembly, call) = fixture(None);
    let Some(CollectionOperation::Append { owner: CollectionMutationOwner::AggregateField { receiver, root, .. } }) =
        collection_operation(&db, call).unwrap()
    else {
        panic!("nested owner proof required")
    };
    let profile = aggregate_field_access(&db, receiver).unwrap().expect("canonical Profile field projection");
    assert_ne!(receiver, profile.receiver, "terminal receiver must not be replaced by the lexical ancestor");
    assert_eq!(beskid_queries::local_slot(&db, profile.receiver).unwrap(), Some(root));
    let stale = AstNodeKey { generation: beskid_queries::SyntaxGenerationId(assembly.generation.0 + 1), ..call };
    assert!(collection_operation(&db, stale).unwrap().is_none());
}

fn reject_untrusted_source(source: &str) {
    let (_root, db, assembly, call) = fixture(Some(source));
    let Some(beskid_queries::CallLowering::Direct(declaration)) = beskid_queries::call_lowering(&db, call).unwrap()
    else {
        panic!("user-owned Append must remain an ordinary direct call");
    };
    assert_eq!(declaration.generation, assembly.generation);
    assert!(declaration.unit.path(&db).ends_with("Src/Core/Collections/Array.bd"));
    assert_eq!(
        collection_operation(&db, call).unwrap(),
        None,
        "a suffix-matching user file must not acquire canonical collection mutation authority"
    );
    let stale = AstNodeKey { generation: beskid_queries::SyntaxGenerationId(assembly.generation.0 + 1), ..call };
    assert_eq!(collection_operation(&db, stale).unwrap(), None);
}

#[test]
fn user_array_path_collision_does_not_gain_intrinsic_authority() {
    reject_untrusted_source(
        "pub T[] Append<T>(mut T[] values, T value) { mut i64 observed = 0; observed = observed + 1; return values; }",
    );
}

#[test]
fn exact_copied_canonical_array_without_physical_authority_is_ordinary_source() {
    reject_untrusted_source(include_str!("../../../corelib/packages/foundation/src/Core/Collections/Array.bd"));
}
