//! Strict documentation attachment controls for generated and user-authored source.
use beskid_analysis::services::parse_program_with_source_name_and_diagnostics;
use beskid_analysis::syntax::Node;

#[test]
fn v06_indented_documentation_retains_variant_text_and_source_bounds() {
    let source = "pub enum Choice {\n    /// First line.\n    /// Second line.\n    Value,\n}\n";
    let parsed = parse_program_with_source_name_and_diagnostics("Choice.bd", source).unwrap();
    assert!(!parsed.recovered && parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Node::EnumDefinition(definition) = &parsed.program.node.items[0].node else { panic!("expected enum") };
    let documentation = definition.node.variant_docs[0].as_ref().expect("attached variant documentation");
    assert!(documentation.normalized_source.contains("First line."));
    assert!(documentation.normalized_source.contains("Second line."));
    let attached_source = &source[documentation.span.start..documentation.span.end];
    assert!(attached_source.contains("/// First line."));
    assert!(attached_source.contains("/// Second line."));
    assert!(!attached_source.contains("Value,"));
}

#[test]
fn v06_tab_indented_documentation_preserves_record_field_metadata() {
    let source = "pub type Record {\n\t/// First field line.\n\t/// Second field line.\n\tu32 value,\n}\n";
    let parsed = parse_program_with_source_name_and_diagnostics("Record.bd", source).unwrap();
    assert!(!parsed.recovered && parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Node::TypeDefinition(definition) = &parsed.program.node.items[0].node else { panic!("expected record") };
    let documentation = definition.node.field_docs[0].as_ref().expect("attached field documentation");
    assert!(documentation.normalized_source.contains("First field line."));
    assert!(documentation.normalized_source.contains("Second field line."));
    assert_eq!(definition.node.fields.len(), 1);
}

#[test]
fn v06_documentation_keeps_markdown_indentation_and_crlf_source_bounds() {
    let source = "pub enum Choice {\r\n    /// Summary.\r\n\t///     indented example\r\n    Value,\r\n}\r\n";
    let parsed = parse_program_with_source_name_and_diagnostics("Choice.bd", source).unwrap();
    assert!(!parsed.recovered && parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Node::EnumDefinition(definition) = &parsed.program.node.items[0].node else { panic!("expected enum") };
    let documentation = definition.node.variant_docs[0].as_ref().expect("attached variant documentation");
    assert_eq!(documentation.normalized_source, "Summary.\n    indented example");
    let attached = &source[documentation.span.start..documentation.span.end];
    assert!(attached.contains("\t///     indented example\r\n"));
    assert!(!attached.contains("Value,"));
}
