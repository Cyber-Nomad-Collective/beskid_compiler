//! Discovery retains exact registered contracts; leaf names are not native ABI authority.
#[path = "codegen_input/support.rs"]
#[allow(unused_imports, dead_code)]
mod support;
use beskid_analysis::syntax::{ContractDefinition, TypeDefinition};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{AstNodeKey, SyntaxGenerationId, type_contract_declarations, type_contract_implementation};

#[test]
fn declared_contract_discovery_preserves_exact_keys_and_rejects_stale_types() {
    let (db, typed, root, _) = support::input_fixture_with_source(
        "pub contract First { unit Run(); } pub contract Second { unit Run(); } \
         pub type Tool : First { pub unit Run() { return; } } \
         pub type Other : Second { pub unit Run() { return; } } \
         pub contract Typed { unit Consume(string value); } \
         pub type Matching : Typed { pub unit Consume(string value) { return; } } \
         pub type Wrong : Typed { pub unit Consume(u8[] value) { return; } }",
    );
    let index = typed.assembly.entry_syntax_index();
    let program = &typed.assembly.entry_unit().program;
    let type_key = |name: &str| AstNodeKey {
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
    let contract_key = |name: &str| AstNodeKey {
        node: index
            .ids_of_kind(NodeKind::ContractDefinition)
            .find(|node| {
                index
                    .node_at(program, *node)
                    .and_then(|node| node.of::<ContractDefinition>())
                    .is_some_and(|definition| definition.name.node.name == name)
            })
            .unwrap(),
        ..root
    };
    assert_eq!(type_contract_declarations(&db, type_key("Tool")).unwrap().unwrap().as_ref(), &[contract_key("First")]);
    assert_eq!(
        type_contract_declarations(&db, type_key("Other")).unwrap().unwrap().as_ref(),
        &[contract_key("Second")]
    );
    let stale = AstNodeKey { generation: SyntaxGenerationId(root.generation.0 + 1), ..type_key("Tool") };
    assert!(type_contract_declarations(&db, stale).unwrap().is_none());
    assert!(type_contract_declarations(&db, contract_key("First")).unwrap().is_none());
    let methods = type_contract_implementation(&db, type_key("Tool"), contract_key("First")).unwrap().unwrap();
    assert_eq!(methods.len(), 1);
    assert_eq!(index.kind(methods[0].0.node), Some(NodeKind::ContractMethodSignature));
    assert_eq!(index.kind(methods[0].1.node), Some(NodeKind::MethodDefinition));
    assert!(type_contract_implementation(&db, type_key("Tool"), contract_key("Second")).is_err());
    assert!(type_contract_implementation(&db, type_key("Matching"), contract_key("Typed")).unwrap().is_some());
    assert!(
        type_contract_implementation(&db, type_key("Wrong"), contract_key("Typed")).is_err(),
        "equal pointer ABI cannot authorize different SDK source types"
    );
    assert!(type_contract_implementation(&db, stale, contract_key("First")).unwrap().is_none());
}
