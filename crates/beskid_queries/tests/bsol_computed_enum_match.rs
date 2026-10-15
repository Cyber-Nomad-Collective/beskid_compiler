//! Checked enum provenance for the native BSOL diagnostic-stage trigger.
use beskid_analysis::projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SourceUnitId, SyntaxGenerationId, build_typed_program, enum_match,
    project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

#[test]
fn indexed_record_enum_match_retains_layout_and_generation_authority() {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().join("src");
    std::fs::create_dir(&source_root).unwrap();
    let path = source_root.join("Main.bd");
    std::fs::write(&path, "pub enum Stage { Profile, Other, }\npub type Diagnostic { pub Stage stage, }\npub unit AssertBool(bool value) {}\npub unit Verify(Diagnostic[] diagnostics) { AssertBool(match diagnostics[0].stage { Stage::Profile => true, _ => false }); }\n").unwrap();
    let plan = CompilePlan {
        project_root: root.path().to_owned(),
        manifest_path: root.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let generation = assembly.generation;
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "computed-enum".into()).unwrap();
    build_typed_program(&mut db, project, generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let node = assembly.entry_syntax_index().ids_of_kind(NodeKind::MatchExpression).next().unwrap();
    let key = AstNodeKey { unit, generation, node };
    let fact = enum_match(&db, key).expect("indexed field must retain canonical enum identity").expect("match fact");
    assert_eq!(fact.layout.variants.len(), 2);
    assert_eq!(fact.arms.len(), 2);
    assert_eq!(enum_match(&db, AstNodeKey { generation: SyntaxGenerationId(generation.0 + 1), ..key }), Ok(None));
}
