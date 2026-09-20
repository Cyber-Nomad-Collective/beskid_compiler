use super::support::{key, setup};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, CallLowering, call_lowering, generic_call_specialization, generic_call_specialization_in_environment,
    generic_call_specialization_instance, generic_specialization_identity, item_abi_signature,
};

#[test]
fn function_generic_name_shadows_outer_contract() {
    let source = "contract Reader { i64 Read(); } Reader Identity<Reader>(Reader value) { return value; } unit Main() { Identity<i64>(1_i64); }";
    let (db, _, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let instance = generic_call_specialization(&db, call).expect("generic Reader shadows contract").unwrap();
    assert!(instance.contract_witnesses.is_empty());
    assert_eq!(instance.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
    assert_eq!(instance.signature.result, beskid_queries::SemanticTypeId::I64);
}

#[test]
fn generic_owner_name_shadows_outer_contract_for_method_parameters() {
    let source = "contract Reader { i64 Read(); } type Identity<Reader> { pub Reader Echo(Reader value) { return value; } } unit Main(Identity<i64> identity) { identity.Echo(1_i64); }";
    let (db, _, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let instance = generic_call_specialization(&db, call).expect("owner Reader shadows contract").unwrap();
    assert!(instance.contract_witnesses.is_empty());
    assert_eq!(
        instance.signature.parameters.as_ref(),
        &[beskid_queries::SemanticTypeId::POINTER, beskid_queries::SemanticTypeId::I64]
    );
    assert_eq!(instance.signature.result, beskid_queries::SemanticTypeId::I64);
}

#[test]
fn shadowed_generic_receiver_does_not_acquire_outer_contract_members() {
    for declaration in [
        "unit Inspect<Reader>(Reader value) { value.Read(); }",
        "type Inspector<Reader> { pub unit Inspect(Reader value) { value.Read(); } }",
    ] {
        let (db, _, unit, generation, index) = setup(&format!("contract Reader {{ i64 Read(); }} {declaration}"));
        let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
        assert!(
            call_lowering(&db, call).is_err(),
            "an unconstrained generic member must fail closed, not inherit a same-named contract's members"
        );
    }
}

#[test]
fn genuine_imported_contract_parameter_still_mints_a_concrete_witness() {
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::parse_program;
    use beskid_analysis::syntax_query::SyntaxIndex;
    use beskid_queries::{BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId};
    use std::{path::PathBuf, sync::Arc};

    let root = PathBuf::from("/tmp/contract-shadowing/src");
    let main_path = root.join("Main.bd");
    let main_source = "use Api.Reader; type Source: Reader { pub i64 Read() { return 17_i64; } } i64 ReadOne(Reader reader) { return reader.Read(); } unit Main() { ReadOne(Source {}); }";
    let main_program = parse_program(main_source).unwrap();
    let contract_source = "pub contract Reader { i64 Read(); }";
    let generation = SyntaxGenerationId(77);
    let index = SyntaxIndex::from_program(&main_program, generation);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                path: main_path.clone(),
                logical_name: "Main.bd".into(),
                source: main_source.into(),
                program: main_program,
            },
            SourceUnit {
                path: root.join("Api/Reader.bd"),
                logical_name: "Api/Reader.bd".into(),
                source: contract_source.into(),
                program: parse_program(contract_source).unwrap(),
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, main_path.clone());
    let project = ProjectSession::new(&db, root.parent().unwrap().to_owned(), main_path, "App".into(), "test".into());
    beskid_queries::build_typed_program(&mut db, project, generation, assembly).unwrap();
    let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let instance = generic_call_specialization_instance(&db, generic_call_specialization(&db, call).unwrap().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(instance.contract_witnesses.len(), 1);
    let method_call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let implementation = generic_call_specialization_in_environment(&db, method_call, &instance).unwrap().unwrap();
    assert_eq!(implementation.declaration, key(unit, generation, &index, NodeKind::MethodDefinition, 0));
}

#[test]
fn array_literals_have_one_pointer_abi_for_scalar_and_managed_elements() {
    for literal in ["[1_u8, 2_u8]", "[\"one\", \"two\"]"] {
        let (db, _, unit, generation, index) = setup(&format!("unit Main() {{ let values = {literal}; }}"));
        let array = key(unit, generation, &index, NodeKind::ArrayLiteralExpression, 0);
        assert_eq!(beskid_queries::abi_type(&db, array).unwrap(), Some(beskid_queries::SemanticTypeId::POINTER));
    }
}

#[test]
fn contract_template_accepts_direct_array_literal_arguments() {
    let source = "contract Reader { i64 Read(); } type Source: Reader { pub i64 Read() { return 1_i64; } } i64 Consume(Reader reader, u8[] bytes) { return reader.Read(); } unit Main() { Consume(Source {}, [1_u8, 2_u8]); }";
    let (db, _, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let instance = generic_call_specialization(&db, call).expect("array literal representation").unwrap();
    assert_eq!(instance.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::POINTER; 2]);
}

#[test]
fn contract_parameters_specialize_independently_by_concrete_source_identity() {
    let source = r#"
contract Reader { i64 Read(); }
type Memory: Reader { pub i64 Read() { return 17_i64; } }
type File: Reader { pub i64 Read() { return 25_i64; } }
i64 ReadBoth(Reader left, Reader right) { return left.Read() + right.Read(); }
unit Main() {
    Memory memory = Memory {};
    File file = File {};
    ReadBoth(memory, file);
    ReadBoth(file, memory);
}

"#;
    let (db, _, unit, generation, index) = setup(source);
    let read_both = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    assert_eq!(item_abi_signature(&db, read_both).expect("resolved contract template"), None);
    let instances = index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit, generation, node })
        .filter(|call| matches!(call_lowering(&db, *call), Ok(Some(CallLowering::Direct(item))) if item == read_both))
        .map(|call| generic_call_specialization(&db, call).unwrap().expect("concrete witnesses"))
        .map(|call| generic_call_specialization_instance(&db, call).unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(instances.len(), 2);
    assert_ne!(generic_specialization_identity(&instances[0]), generic_specialization_identity(&instances[1]));
    let left = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let right = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let memory_read = key(unit, generation, &index, NodeKind::MethodDefinition, 0);
    let file_read = key(unit, generation, &index, NodeKind::MethodDefinition, 1);
    for (instance, expected) in instances.iter().zip([[memory_read, file_read], [file_read, memory_read]]) {
        for (call, method) in [left, right].into_iter().zip(expected) {
            assert_eq!(
                generic_call_specialization_in_environment(&db, call, instance).unwrap().unwrap().declaration,
                method
            );
        }
    }
}

#[test]
fn contract_specialization_rejects_missing_private_and_mismatched_implementations() {
    for (implementation, expected) in [
        ("type Source { pub i64 Read() { return 1_i64; } }", "missing contract conformance"),
        ("type Source: Reader { }", "missing contract method implementation"),
        ("type Source: Reader { i64 Read() { return 1_i64; } }", "private"),
        ("type Source: Reader { pub i32 Read() { return 1; } }", "signature mismatch"),
    ] {
        let source = format!(
            "contract Reader {{ i64 Read(); }} {implementation} i64 ReadOne(Reader reader) {{ return reader.Read(); }} unit Main() {{ ReadOne(Source {{}}); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let call = key(unit, generation, &index, NodeKind::CallExpression, 1);
        let error = generic_call_specialization(&db, call).expect_err("invalid conformance cannot mint a witness");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn contract_body_cannot_see_implementation_only_members() {
    let source = "contract Reader { i64 Read(); } type Source: Reader { pub i64 Read() { return 1_i64; } pub unit Reset() {} } unit Consume(Reader reader) { reader.Reset(); } unit Main() { Consume(Source {}); }";
    let (db, _, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let error = call_lowering(&db, call).expect_err("contract scope excludes Reset");
    assert!(error.to_string().contains("contract member is not visible: Reset"), "{error}");
}

#[test]
fn contract_template_does_not_hide_unknown_noncontract_parameter_types() {
    let (db, _, unit, generation, index) =
        setup("contract Reader { i64 Read(); } unit Invalid(Reader source, Missing unknown) {}");
    let invalid = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    assert!(item_abi_signature(&db, invalid).is_err(), "only a proven contract parameter can defer its ABI");
}

#[test]
fn reader_result_byte_index_has_the_source_element_abi_for_integer_conversion() {
    let (db, _, unit, generation, index) = setup("i64 Value(u8[] bytes) { return i64(bytes[0]); }");
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let conversion = beskid_queries::primitive_numeric_conversion(&db, call).expect("indexed byte conversion").unwrap();
    assert_eq!(conversion.from, beskid_queries::SemanticTypeId::U8);
    assert_eq!(conversion.to, beskid_queries::SemanticTypeId::I64);
}

#[test]
fn indexed_managed_element_keeps_string_identity_and_rejects_integer_conversion() {
    let (db, _, unit, generation, index) = setup("i64 Invalid(string[] bytes) { return i64(bytes[0]); }");
    let element = key(unit, generation, &index, NodeKind::IndexExpression, 0);
    assert_eq!(beskid_queries::abi_type(&db, element).unwrap(), Some(beskid_queries::SemanticTypeId::STRING));
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    assert!(beskid_queries::primitive_numeric_conversion(&db, call).is_err());
}

#[test]
fn contract_specialization_forwards_independent_witnesses_through_generic_calls() {
    let source = r#"
contract Reader { i64 Read(); }
type Memory: Reader { pub i64 Read() { return 17_i64; } }
type File: Reader { pub i64 Read() { return 25_i64; } }
i64 Inner<T>(Reader left, Reader right, T value) { return left.Read() + right.Read(); }
i64 Outer<T>(Reader left, Reader right, T value) { return Inner<T>(right, left, value); }
unit Main() { Outer<i64>(Memory {}, File {}, 0_i64); }
"#;
    let (db, _, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 3);
    let outer = generic_call_specialization_instance(&db, generic_call_specialization(&db, call).unwrap().unwrap())
        .unwrap()
        .unwrap();
    let nested = key(unit, generation, &index, NodeKind::CallExpression, 2);
    let inner = generic_call_specialization_in_environment(&db, nested, &outer).unwrap().unwrap();
    assert_eq!(inner.contract_witnesses.len(), 2);
    let left = generic_call_specialization_in_environment(
        &db,
        key(unit, generation, &index, NodeKind::CallExpression, 0),
        &inner,
    )
    .unwrap()
    .unwrap();
    assert_eq!(left.declaration, key(unit, generation, &index, NodeKind::MethodDefinition, 1));
}
