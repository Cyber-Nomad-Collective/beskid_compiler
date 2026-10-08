//! Real registered assembly facts required by serialization and Rust Glue.
use beskid_analysis::{
    projects::{AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer},
    syntax_query::NodeKind,
};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SemanticTypeId, SourceUnitId, abi_type, build_typed_program, item_signature,
    project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

#[test]
fn required_primitive_signatures_literals_and_layouts_are_canonical() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("Src");
    std::fs::create_dir(&src).unwrap();
    let path = src.join("Main.bd");
    std::fs::write(
        &path,
        r#"
pub i8 SignedByte(i8 value) { return -128_i8; }
pub i16 SignedShort(i16 value) { return -32768_i16; }
pub u16 UnsignedShort(u16 value) { return 65535_u16; }
pub u64 UnsignedLong(u64 value) { return 18446744073709551615_u64; }
pub f32 Float32(f32 value) { return 1.5_f32; }
"#,
    )
    .unwrap();
    let plan = CompilePlan {
        project_root: root.path().to_owned(),
        manifest_path: root.path().join("Fixture.bproj"),
        project_name: "Fixture".into(),
        source_root: src,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "primitive-profile".into())
            .unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let index = assembly.entry_syntax_index();
    let expected = [
        (SemanticTypeId::I8, 1),
        (SemanticTypeId::I16, 2),
        (SemanticTypeId::U16, 2),
        (SemanticTypeId::U64, 8),
        (SemanticTypeId::F32, 4),
    ];
    for (position, (ty, size)) in expected.into_iter().enumerate() {
        let key = AstNodeKey {
            unit,
            generation: assembly.generation,
            node: index.ids_of_kind(NodeKind::FunctionDefinition).nth(position).unwrap(),
        };
        let signature = item_signature(&db, key).unwrap().expect("canonical signature");
        assert_eq!(&*signature.parameters, &[ty]);
        assert_eq!(signature.result, ty);
        let layout = ty.scalar_abi_layout(64).expect("scalar layout");
        assert_eq!(layout.size, size);
        assert_eq!(layout.alignment, size);
        assert!(!layout.is_pointer);
        let literal = AstNodeKey {
            unit,
            generation: assembly.generation,
            node: index.ids_of_kind(NodeKind::Literal).nth(position).unwrap(),
        };
        assert_eq!(abi_type(&db, literal).unwrap(), Some(ty));
    }
    assert_ne!(SemanticTypeId::I8, SemanticTypeId::U8);
    assert_ne!(SemanticTypeId::I16, SemanticTypeId::U16);
    assert_ne!(SemanticTypeId::I64, SemanticTypeId::U64);
    assert_ne!(SemanticTypeId::F32, SemanticTypeId::F64);
}
