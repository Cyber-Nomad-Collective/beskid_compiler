//! Empty call-argument arrays derive ABI only from the expected type of the resolved callee
//! parameter: the declared `T[]` syntax in the callee's own unit, or the explicit terminal type
//! argument when `T` is one of the callee's generic parameters.
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SemanticTypeId, SourceUnitId, build_typed_program,
    empty_array_literal_element_abi_type, project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

fn fixture(source: &str, modules: &[(&str, &str)]) -> (tempfile::TempDir, BeskidDatabase, Arc<ProgramAssembly>, Vec<AstNodeKey>) {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("src");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("Main.bd");
    std::fs::write(&path, source).unwrap();
    for (relative, text) in modules {
        let module = root.join(relative);
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        std::fs::write(module, text).unwrap();
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
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "empty-call-array".into())
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
fn direct_non_generic_call_arguments_take_declared_parameter_element_abi() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "type Item {} unit Named(string[] names, Item[] arguments) {} unit Main() { Named([], []); }",
        &[],
    );
    assert_eq!(arrays.len(), 2);
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(SemanticTypeId::STRING)));
    assert_eq!(
        empty_array_literal_element_abi_type(&db, arrays[1]),
        Ok(Some(SemanticTypeId::POINTER)),
        "a nominal element parameter never inherits the adjacent string slot"
    );
}

#[test]
fn module_qualified_callee_resolves_qualified_element_type_in_its_own_unit() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "use Probe.Generation; unit Main() { Generation.Record(0_i64, []); }",
        &[
            ("Probe/Read.bd", "pub type ContainerSite { pub i64 slot, }"),
            ("Probe/Generation.bd", "use Probe.Read; pub unit Record(i64 count, Read.ContainerSite[] containers) {}"),
        ],
    );
    assert_eq!(arrays.len(), 1);
    assert_eq!(
        empty_array_literal_element_abi_type(&db, arrays[0]),
        Ok(Some(SemanticTypeId::POINTER)),
        "`Read.ContainerSite` is resolved through the callee unit's imports, not the caller's"
    );
}

#[test]
fn explicit_generic_call_substitutes_the_callee_type_parameter() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "type Item {} unit Append<T>(T[] values, T value) {} unit Main() { Append<u8>([], 1_u8); Append<Item>([], Item {}); }",
        &[],
    );
    assert_eq!(arrays.len(), 2);
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(SemanticTypeId::U8)));
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[1]), Ok(Some(SemanticTypeId::POINTER)));
}

#[test]
fn nested_call_argument_keeps_its_own_callee_context() {
    let (_temporary, db, _assembly, arrays) = fixture(
        "type Item {} Item Named(string[] names, Item[] arguments) { return Item {}; } unit Append<T>(T[] values, T value) {} unit Main() { Append<Item>([], Named([], [])); }",
        &[],
    );
    assert_eq!(arrays.len(), 3);
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(SemanticTypeId::POINTER)));
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[1]), Ok(Some(SemanticTypeId::STRING)));
    assert_eq!(empty_array_literal_element_abi_type(&db, arrays[2]), Ok(Some(SemanticTypeId::POINTER)));
}

#[test]
fn inferred_generic_scalar_slot_and_unresolved_callee_have_no_array_authority() {
    for source in [
        "unit Append<T>(T[] values, T value) {} unit Main() { Append([], 1_u8); }",
        "unit Scalar(i64 value) {} unit Main() { Scalar([]); }",
        "unit Main() { Missing([]); }",
    ] {
        let (_temporary, db, _assembly, arrays) = fixture(source, &[]);
        assert_eq!(arrays.len(), 1);
        assert!(
            !matches!(empty_array_literal_element_abi_type(&db, arrays[0]), Ok(Some(_))),
            "empty literal must never gain guessed element type: {source}"
        );
    }
}
