//! Empty enum payload arrays derive ABI only from the selected declared payload slot.
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SemanticTypeId, SourceUnitId, build_typed_program,
    empty_array_literal_element_abi_type, project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

fn fixture(source: &str, imported: bool) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>, Vec<AstNodeKey>) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("src");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("Main.bd");
    std::fs::write(&path, source).unwrap();
    if imported {
        let module = root.join("Probe/Values.bd");
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        std::fs::write(module,"pub enum DataValue { Unit, Optional(bool present, DataValue[] payload), Bytes(u8[] payload), } pub enum Container<T> { Items(T[] payload), }").unwrap();
    }
    let plan = CompilePlan {
        project_root: temporary.path().to_owned(),
        manifest_path: temporary.path().join("Host.bproj"),
        project_name: "Host".into(),
        source_root: root,
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
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "empty-enum-array".into())
            .unwrap();
    build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let arrays = assembly
        .entry_syntax_index()
        .ids_of_kind(NodeKind::ArrayLiteralExpression)
        .map(|node| AstNodeKey { unit, generation: assembly.generation, node })
        .collect();
    (temporary, db, assembly, arrays)
}

#[test]
fn enum_optional_recursive_and_byte_slots_supply_exact_element_abi() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "enum DataValue { Unit, Optional(bool present, DataValue[] payload), Bytes(u8[] payload), } unit Main() { DataValue::Optional(false, []); DataValue::Bytes([]); }",
        false,
    );
    assert_eq!(arrays.len(), 2);
    assert_eq!(
        empty_array_literal_element_abi_type(&db, arrays[0]),
        Ok(Some(SemanticTypeId::POINTER)),
        "recursive managed enum element is a pointer"
    );
    assert_eq!(
        empty_array_literal_element_abi_type(&db, arrays[1]),
        Ok(Some(SemanticTypeId::U8)),
        "byte payload never inherits adjacent bool slot"
    );
}

#[test]
fn imported_enum_payload_slots_keep_declared_array_identity() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "use Probe.Values; unit Main() { Values.DataValue::Optional(false, []); Values.DataValue::Bytes([]); }",
        true,
    );
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(SemanticTypeId::POINTER)));
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[1]), Ok(Some(SemanticTypeId::U8)));
}

#[test]
fn explicit_and_contextual_generic_enum_payloads_substitute_element_type() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "enum Container<T> { Items(T[] payload), } unit Main() { Container<u8>::Items([]); Container<i64> value = Container::Items([]); }",
        false,
    );
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(SemanticTypeId::U8)));
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[1]), Ok(Some(SemanticTypeId::I64)));
}

#[test]
fn absent_declaration_nonarray_slot_and_nested_expression_have_no_array_authority() {
    for source in [
        "unit Main() { Missing::Items([]); }",
        "enum Scalar { Value(i64 value), } unit Main() { Scalar::Value([]); }",
        "enum Bytes { Value(u8[] value), } unit Main() { Bytes::Value({ []; }); }",
    ] {
        let (_temporary, db, _assembly, arrays) = fixture(source, false);
        assert_eq!(arrays.len(), 1);
        assert!(
            !matches!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(_))),
            "empty literal must never gain guessed element type: {source}"
        );
    }
}
