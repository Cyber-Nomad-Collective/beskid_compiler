use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::Node;

#[test]
fn scoped_use_is_a_binding_while_module_use_remains_an_import() {
    let source = "use Core.Disposable; Result<unit, DisposeError> Run() { use Resource resource = Open(); return Result::Ok(unit); }";
    let program = parse_program(source).expect("a scoped use binding must parse alongside an import");
    assert!(matches!(program.node.items[0].node, Node::UseDeclaration(_)));
    let Node::Function(function) = &program.node.items[1].node else {
        panic!("expected enclosing function");
    };
    assert_eq!(function.node.body.node.statements.len(), 2);
    let beskid_analysis::syntax::Statement::Use(scoped) = &function.node.body.node.statements[0].node else {
        panic!("scoped ownership must have a distinct AST node");
    };
    assert_eq!(scoped.node.binding.node.name.node.name, "resource");
    let formatted = beskid_analysis::format::format_source("<memory>", source).unwrap();
    let reparsed = parse_program(&formatted).unwrap();
    let Node::Function(function) = &reparsed.node.items[1].node else { panic!("formatted function") };
    assert!(matches!(function.node.body.node.statements[0].node, beskid_analysis::syntax::Statement::Use(_)));
}

#[test]
fn scoped_use_rejects_using_alias_and_missing_initializer() {
    use beskid_analysis::parser::{BeskidParser, Rule};
    use pest::Parser;
    for source in [
        "unit Run() { using Resource resource = Open(); }",
        "unit Run() { use Resource resource; }",
        "unit Run() { use (Resource resource = Open()) {}; }",
    ] {
        assert!(BeskidParser::parse(Rule::Program, source).is_err(), "unexpectedly accepted {source}");
    }
}

#[test]
fn scoped_use_block_header_unambiguously_separates_a_struct_initializer() {
    let source = "use Core.Disposable; Result<unit, DisposeError> Run() { use (Resource resource = Resource {}) { resource.Read(); } return Result::Ok(()); }";
    let program = parse_program(source).expect("block use must parse");
    let Node::Function(function) = &program.node.items[1].node else { panic!("function") };
    let beskid_analysis::syntax::Statement::Use(scoped) = &function.node.body.node.statements[0].node else {
        panic!("one scoped-use AST")
    };
    assert_eq!(scoped.node.binding.node.name.node.name, "resource");
    assert_eq!(scoped.node.body.as_ref().expect("explicit use body").node.statements.len(), 1);
    let formatted = beskid_analysis::format::format_source("<memory>", source).unwrap();
    assert!(formatted.contains("use (Resource resource"));
    assert!(parse_program(&formatted).is_ok());
}

#[test]
fn foundation_publishes_the_canonical_disposable_contract() {
    use beskid_analysis::syntax::{ContractNode, Node, Visibility};

    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corelib/packages/foundation/src/Core/Disposable.bd"),
    )
    .expect("foundation must publish Core.Disposable");
    let program = parse_program(&source).unwrap();
    assert_eq!(program.node.items.len(), 2, "two public declarations with a fully qualified Result type");
    let Node::ContractDefinition(contract) = &program.node.items[0].node else { panic!("Disposable contract") };
    assert_eq!(contract.node.name.node.name, "Disposable");
    assert!(matches!(contract.node.visibility.node, Visibility::Public));
    let [method] = contract.node.items.as_slice() else { panic!("one Disposable member") };
    let ContractNode::MethodSignature(dispose) = &method.node else { panic!("Dispose signature") };
    assert_eq!(dispose.node.name.node.name, "Dispose");
    assert!(dispose.node.parameters.is_empty());
    let result = dispose.node.return_type.as_ref().expect("Dispose returns a Result");
    let result_text: String = source[result.span.start..result.span.end].split_whitespace().collect();
    assert_eq!(result_text, "Core.Results.Result<unit,DisposeError>");
    let Node::EnumDefinition(error) = &program.node.items[1].node else { panic!("DisposeError enum") };
    assert_eq!(error.node.name.node.name, "DisposeError");
    assert!(matches!(error.node.visibility.node, Visibility::Public));
    let variants = error.node.variants.iter().map(|variant| variant.node.name.node.name.as_str()).collect::<Vec<_>>();
    assert_eq!(variants, ["Failed"]);
}
