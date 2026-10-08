//! A call to a function imported from a dependency module must check its argument types
//! exactly like a call inside the unit. `Query.Span(Query.At(root))` in the Serialization Mod
//! passed a `SyntaxQuery` record where `Span` declares a `NodeRef`; `beskid check` reported
//! nothing and the native worker then read the record as a `NodeRef`.
use beskid_analysis::resolve::Resolver;
use beskid_analysis::services::parse_program;
use beskid_analysis::types::checker::TypeChecker;
use beskid_analysis::types::result::TypeError;
use std::path::PathBuf;

fn errors_for(entry_source: &str) -> Vec<TypeError> {
    let nodes_path = PathBuf::from("/tmp/v06-dependency-args/Beskid/Syntax/Nodes.bd");
    let query_path = PathBuf::from("/tmp/v06-dependency-args/Beskid/Compiler/Query.bd");
    let entry_path = PathBuf::from("/tmp/v06-dependency-args/Driver.bd");
    let nodes = parse_program("pub type NodeRef { pub u64 issuer, pub u64 node, } pub type NodeSpan { pub u64 start, pub u64 end, }").unwrap();
    let query = parse_program(
        "use Beskid.Syntax.Nodes; pub type SyntaxQuery { pub Beskid.Syntax.Nodes.NodeRef start, pub u64 bound, } \
         pub SyntaxQuery At(Beskid.Syntax.Nodes.NodeRef root) { return SyntaxQuery { start: root, bound: 1_u64 }; } \
         pub Beskid.Syntax.Nodes.NodeSpan Span(Beskid.Syntax.Nodes.NodeRef node) { return Beskid.Syntax.Nodes.NodeSpan { start: node.node, end: node.node }; }",
    )
    .unwrap();
    let mut entry = parse_program(entry_source).unwrap();
    let mut resolver = Resolver::new();
    resolver.collect_program_in_module(&nodes, &["Beskid".into(), "Syntax".into(), "Nodes".into()], Some(&nodes_path));
    resolver.collect_program_in_module(&query, &["Beskid".into(), "Compiler".into(), "Query".into()], Some(&query_path));
    let resolution = resolver.resolve_program(&entry).expect("imports and declarations resolve");
    let (_, errors) = TypeChecker::check_entry(
        &mut entry,
        &resolution,
        &[&nodes, &query],
        Some(&[nodes_path, query_path]),
        Some(entry_path),
        true,
        None,
        None,
        None,
        None,
    );
    errors
}

#[test]
fn imported_function_rejects_a_record_argument_of_the_wrong_nominal_type() {
    let errors = errors_for(
        "use Beskid.Compiler.Query; use Beskid.Syntax.Nodes; pub u64 Main(NodeRef root) { NodeSpan span = Query.Span(Query.At(root)); return span.start; }",
    );
    assert!(
        errors.iter().any(|error| matches!(error, TypeError::TypeMismatch { .. } | TypeError::CallArgumentMismatch { .. })),
        "a SyntaxQuery argument for an imported NodeRef parameter must be a type error: {errors:?}"
    );
}

#[test]
fn imported_function_accepts_the_declared_nominal_argument() {
    let errors = errors_for(
        "use Beskid.Compiler.Query; use Beskid.Syntax.Nodes; pub u64 Main(NodeRef root) { NodeSpan span = Query.Span(root); return span.start + Query.At(root).bound; }",
    );
    assert!(errors.is_empty(), "the declared NodeRef argument must type-check: {errors:?}");
}
