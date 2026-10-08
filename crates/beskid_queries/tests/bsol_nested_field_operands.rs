//! Reproduce the native BSOL field-span arithmetic through real registered syntax.
use beskid_analysis::projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer};
use beskid_analysis::syntax::{BinaryExpression, PathExpression};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SemanticTypeId, SourceUnitId, SyntaxGenerationId, abi_type, aggregate_field_access,
    build_typed_program, project_session_for_planned_syntax_assembly, value_abi_type,
};
use std::sync::Arc;

#[test]
fn bsol_nested_span_arithmetic_preserves_canonical_operand_abi_facts() {
    let original = include_str!("../../../corelib/beskid_corelib/tests/corelib_tests/src/bsol/SyntaxTests.bd");
    assert!(
        original.contains("value.span.end - value.span.start"),
        "regression must remain bound to the native BSOL trigger"
    );
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.build/compiler/semantic-query-fixtures");
    std::fs::create_dir_all(&root).unwrap();
    let root =
        tempfile::Builder::new().prefix("bsol-field-operands-").tempdir_in(root.canonicalize().unwrap()).unwrap();
    let source_root = root.path().join("Src");
    std::fs::create_dir_all(source_root.join("Core/Bsol")).unwrap();
    std::fs::write(
        source_root.join("Core/Bsol/Syntax.bd"),
        include_str!("../../../corelib/packages/bsol/src/Core/Bsol/Syntax.bd"),
    )
    .unwrap();
    let path = source_root.join("Main.bd");
    let source =
        "use Core.Bsol.Syntax;\npub i64 SpanLength(Syntax.Node value) { return value.span.end - value.span.start; }";
    std::fs::write(&path, source).unwrap();
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
    assert!(
        assembly.units.iter().any(|unit| unit.path.ends_with("Core/Bsol/Syntax.bd")),
        "the real imported syntax module must be assembled"
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "bsol-field-operands".into())
            .unwrap();
    build_typed_program(&mut db, project, generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let index = assembly.entry_syntax_index();
    let program = &assembly.entry_unit().program;
    for terminal in ["end", "start"] {
        let node = index
            .ids_of_kind(NodeKind::PathExpression)
            .find(|id| {
                index.node_at(program, *id).and_then(|node| node.of::<PathExpression>()).is_some_and(|path| {
                    path.path.node.segments.iter().map(|part| part.node.name.node.name.as_str()).collect::<Vec<_>>()
                        == ["value", "span", terminal]
                })
            })
            .unwrap();
        let key = AstNodeKey { unit, generation, node };
        assert_eq!(abi_type(&db, key), Ok(Some(SemanticTypeId::I64)), "canonical path fact is the working control");
        let field = aggregate_field_access(&db, key).unwrap().unwrap();
        assert_eq!(
            field.declaration.unit.path(&db),
            &assembly.units.iter().find(|unit| unit.path.ends_with("Core/Bsol/Syntax.bd")).unwrap().path
        );
    }
    let binary = index.ids_of_kind(NodeKind::BinaryExpression).next().unwrap();
    let expression = index.node_at(program, binary).unwrap().of::<BinaryExpression>().unwrap();
    for operand in [&*expression.left, &*expression.right] {
        let node = index.direct_child_id(program, binary, DynNodeRef::from(operand)).unwrap();
        let key = AstNodeKey { unit, generation, node };
        assert_eq!(
            value_abi_type(&db, key),
            Ok(Some(SemanticTypeId::I64)),
            "expression wrapper must consume the same canonical nested field fact"
        );
        assert_eq!(
            value_abi_type(&db, AstNodeKey { generation: SyntaxGenerationId(generation.0 + 1), ..key }),
            Ok(None),
            "stale operand keys retain no authority"
        );
    }
    assert_eq!(
        value_abi_type(&db, AstNodeKey { unit, generation, node: binary }),
        Ok(Some(SemanticTypeId::I64)),
        "native arithmetic must retain i64 field types"
    );
}
