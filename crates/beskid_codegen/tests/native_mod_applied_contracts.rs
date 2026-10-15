//! Applied contracts retain source arguments before any target ABI comparison.
#[path = "codegen_input/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;
use beskid_analysis::syntax::TypeDefinition;
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, SyntaxGenerationId, type_applied_contract_implementation, type_contract_applications,
};

#[test]
fn applied_contract_witness_substitutes_exact_source_types_and_rejects_stale_owner() {
    let (db, typed, root, _) = support::input_fixture_with_source(
        "pub contract Applied<T> { T Rewrite(T value); } \
         pub type Matching : Applied<string> { pub string Rewrite(string value) { return value; } } \
         pub type Wrong : Applied<string> { pub u8[] Rewrite(u8[] value) { return value; } } \
         pub type Different : Applied<u8[]> { pub u8[] Rewrite(u8[] value) { return value; } }",
    );
    let index = typed.assembly.entry_syntax_index();
    let program = &typed.assembly.entry_unit().program;
    let key = |name: &str| AstNodeKey {
        node: index
            .ids_of_kind(NodeKind::TypeDefinition)
            .find(|node| {
                index
                    .node_at(program, *node)
                    .and_then(|node| node.of::<TypeDefinition>())
                    .is_some_and(|definition| definition.name.node.name == name)
            })
            .unwrap(),
        ..root
    };
    let applications = type_contract_applications(&db, key("Matching")).unwrap().unwrap();
    assert_eq!(applications.len(), 1);
    assert_eq!(applications[0].argument_count(), 1);
    let witness = type_applied_contract_implementation(&db, key("Matching"), &applications[0]).unwrap().unwrap();
    assert_eq!(witness.len(), 1);
    let wrong = type_contract_applications(&db, key("Wrong")).unwrap().unwrap();
    assert!(
        type_applied_contract_implementation(&db, key("Wrong"), &wrong[0]).is_err(),
        "string and u8[] share pointer ABI but not applied source identity"
    );
    assert!(
        type_applied_contract_implementation(&db, key("Different"), &applications[0]).is_err(),
        "a witness issued to one owner cannot authorize another applied contract"
    );
    let stale = AstNodeKey { generation: SyntaxGenerationId(root.generation.0 + 1), ..key("Matching") };
    assert!(type_applied_contract_implementation(&db, stale, &applications[0]).unwrap().is_none());
}

#[test]
fn instance_factory_argument_retains_exact_nominal_receiver_identity() {
    let (db, typed, root, _) = support::input_fixture_with_source(
        "pub type State { i32 value, } pub type Foreign { i32 value, } \
         pub contract Factory<T> { T Create(); } \
         pub type Good : Factory<State> { pub State Create() { return State { value: 7 }; } }",
    );
    let index = typed.assembly.entry_syntax_index();
    let program = &typed.assembly.entry_unit().program;
    let key = |name: &str| AstNodeKey {
        node: index
            .ids_of_kind(NodeKind::TypeDefinition)
            .find(|node| {
                index
                    .node_at(program, *node)
                    .and_then(|node| node.of::<TypeDefinition>())
                    .is_some_and(|definition| definition.name.node.name == name)
            })
            .unwrap(),
        ..root
    };
    let application = type_contract_applications(&db, key("Good")).unwrap().unwrap()[0].clone();
    assert!(beskid_queries::applied_contract_argument_is_type(&db, &application, 0, key("State")).unwrap());
    assert!(
        !beskid_queries::applied_contract_argument_is_type(&db, &application, 0, key("Foreign")).unwrap(),
        "equal pointer layouts cannot authorize another nominal receiver"
    );
    let stale = AstNodeKey { generation: SyntaxGenerationId(root.generation.0 + 1), ..key("State") };
    assert!(beskid_queries::applied_contract_argument_is_type(&db, &application, 0, stale).is_err());
    assert!(beskid_queries::applied_contract_argument_is_type(&db, &application, 1, key("State")).is_err());
}

#[test]
fn enum_applied_contract_uses_exact_impl_receiver_and_nominal_argument() {
    use beskid_analysis::syntax::EnumDefinition;
    let (db, typed, root, _) = support::input_fixture_with_source(
        "pub contract Applied<T> { T Rewrite(T value); } \
         pub enum Choice { Empty, } \
         impl Choice : Applied<Choice> { pub Choice Rewrite(Choice value) { return value; } }",
    );
    let index = typed.assembly.entry_syntax_index();
    let node = index.ids_of_kind(NodeKind::EnumDefinition).find(|node|
        index.node_at(&typed.assembly.entry_unit().program, *node)
            .and_then(|node| node.of::<EnumDefinition>())
            .is_some_and(|definition| definition.name.node.name == "Choice")).unwrap();
    let key = AstNodeKey { node, ..root };
    let applications = type_contract_applications(&db, key).unwrap()
        .expect("registered enum must expose its actual impl conformance");
    assert_eq!(applications.len(), 1);
    assert_eq!(type_applied_contract_implementation(&db, key, &applications[0])
        .unwrap().unwrap().len(), 1);
    assert!(beskid_queries::applied_contract_argument_is_type(&db, &applications[0], 0, key).unwrap());
}
