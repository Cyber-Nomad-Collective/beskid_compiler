use beskid_analysis::services::parse_program_with_source_name_and_diagnostics;
use beskid_analysis::syntax::Node;

#[test]
fn v06_generic_impl_retains_receiver_conformance_and_bounds() {
    let source = "impl<T, E> Item<T> : Serializable<E> where E: Encoder { pub unit Encode(E encoder) {} }";
    let parsed = parse_program_with_source_name_and_diagnostics("GenericImpl.bd", source).unwrap();
    assert!(!parsed.recovered && parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Node::ImplBlock(block) = &parsed.program.node.items[0].node else { panic!("impl expected") };
    assert_eq!(block.node.generics.iter().map(|item| item.node.name.as_str()).collect::<Vec<_>>(), ["T", "E"]);
    assert_eq!(block.node.where_bounds.len(), 1);
    assert_eq!(block.node.where_bounds[0].parameter.node.name, "E");
    assert_eq!(block.node.conformances.len(), 1);
}

#[test]
fn v06_payload_enum_retains_structural_attributes() {
    let source = "[Serialize] pub enum Packet<T> { Empty, Value(T value) }";
    let parsed = parse_program_with_source_name_and_diagnostics("Payload.bd", source).unwrap();
    assert!(!parsed.recovered && parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Node::EnumDefinition(definition) = &parsed.program.node.items[0].node else { panic!("enum expected") };
    assert_eq!(definition.node.attributes.len(), 1);
    assert_eq!(definition.node.generics.len(), 1);
    assert_eq!(definition.node.variants.len(), 2);
}

#[test]
fn v06_generic_impl_and_enum_attributes_survive_formatter_roundtrip() {
    let source = "[Serialize] pub enum Packet<T> { Empty, Value(T value) } \
        impl<T, E> Packet<T> : Serializable<E> where E: Encoder { pub unit Encode(E encoder) {} }";
    let parsed = parse_program_with_source_name_and_diagnostics("Roundtrip.bd", source).unwrap();
    assert!(!parsed.recovered && parsed.diagnostics.is_empty());
    let output = beskid_analysis::format::format_program(&parsed.program).unwrap();
    let reparsed = parse_program_with_source_name_and_diagnostics("Roundtrip.bd", &output).unwrap();
    assert!(!reparsed.recovered && reparsed.diagnostics.is_empty(), "{output}");
    let Node::EnumDefinition(definition) = &reparsed.program.node.items[0].node else { panic!("enum expected") };
    assert_eq!(definition.node.attributes.len(), 1);
    let Node::ImplBlock(block) = &reparsed.program.node.items[1].node else { panic!("impl expected") };
    assert_eq!(block.node.generics.len(), 2);
    assert_eq!(block.node.where_bounds.len(), 1);
    assert_eq!(block.node.methods.len(), 1);
    assert_eq!(beskid_analysis::format::format_program(&reparsed.program).unwrap(), output);
}
