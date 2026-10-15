//! Actual BSOL indexed type metadata must retain ABI authority at call operands.
use beskid_analysis::projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type, build_typed_program,
    call_arguments, project_session_for_planned_syntax_assembly, value_abi_type,
};
use std::sync::Arc;

#[test]
fn indexed_member_call_operand_retains_canonical_string_abi() {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().canonicalize().unwrap().join("src");
    std::fs::create_dir(&source_root).unwrap();
    let path = source_root.join("Main.bd");
    std::fs::write(
        &path,
        r#"
pub type TypeExpression { pub string targetRule, }
pub type Field { pub i64 typeIndex, }
pub type Rule { pub Field[] fields, }
pub type Profile { pub TypeExpression[] types, }
pub unit AssertEqual<T>(T left, T right) {}
pub unit Verify(Profile profile, Rule config) {
    AssertEqual<string>(profile.types[config.fields[0].typeIndex].targetRule, "item");
}
"#,
    )
    .unwrap();
    let plan = CompilePlan {
        project_root: source_root.parent().unwrap().to_owned(),
        manifest_path: source_root.parent().unwrap().join("Host.bproj"),
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
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "indexed-member".into()).unwrap();
    build_typed_program(&mut db, project, generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let node = assembly.entry_syntax_index().ids_of_kind(NodeKind::CallExpression).next().unwrap();
    let call = AstNodeKey { unit, generation, node };
    let arguments = call_arguments(&db, call).unwrap().unwrap();
    let operand = arguments[0];
    assert_eq!(abi_type(&db, arguments[1]), Ok(Some(SemanticTypeId::STRING)), "literal control");
    let index = assembly.entry_syntax_index();
    let expression = index.node_at(&assembly.entry_unit().program, operand.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::Expression>()).unwrap();
    let beskid_analysis::syntax::Expression::Member(member) = expression else {
        panic!("fixture requires an indexed member expression wrapper");
    };
    let member_node = index.direct_child_id(&assembly.entry_unit().program, operand.node,
        beskid_analysis::syntax_query::DynNodeRef::from(member)).unwrap();
    assert_eq!(abi_type(&db, AstNodeKey { node: member_node, ..operand }),
        Ok(Some(SemanticTypeId::STRING)), "canonical member node control");
    assert_eq!(abi_type(&db, operand), Ok(Some(SemanticTypeId::STRING)), "indexed member ABI");
    assert_eq!(value_abi_type(&db, operand), Ok(Some(SemanticTypeId::STRING)), "canonical operand fact");
    let stale = AstNodeKey { generation: SyntaxGenerationId(generation.0 + 1), ..operand };
    assert_eq!(abi_type(&db, stale), Ok(None));
    assert_eq!(value_abi_type(&db, stale), Ok(None));
}
