use beskid_analysis::analysis::SemanticIssueKind;
use beskid_analysis::services::analyze_program;

#[test]
fn async_keyword_reserved_diagnostic() {
    let source = "i64 main() { async x = 1; return 0; }\n";
    let diags = analyze_program(std::path::Path::new("test.bd"), source).expect("analyze");
    assert!(
        diags.iter().any(|d| d.code.as_deref() == Some("E1226")),
        "expected E1226 for async, got: {diags:?}"
    );
}

#[test]
fn await_keyword_reserved_diagnostic() {
    let source = "i64 main() { let x = await; return 0; }\n";
    let diags = analyze_program(std::path::Path::new("test.bd"), source).expect("analyze");
    assert!(
        diags.iter().any(|d| d.code.as_deref() == Some("E1227")),
        "expected E1227 for await, got: {diags:?}"
    );
}

#[test]
fn join_would_deadlock_diagnostic_codes_are_stable() {
    let issue = SemanticIssueKind::JoinWouldDeadlock;
    assert_eq!(issue.code(), "E1224");
    assert!(issue.message().contains("ancestor"));
}

#[test]
fn spawn_expression_parses() {
    use beskid_analysis::services::parse_program;
    let source = "unit child() { }\nunit main() { spawn child(); }\n";
    let program = parse_program(source).expect("parse spawn");
    use beskid_analysis::syntax::Expression;
    let has_spawn = program
        .node
        .items
        .iter()
        .any(|item| {
            if let beskid_analysis::syntax::Node::Function(def) = &item.node {
                def.node
                    .body
                    .node
                    .statements
                    .iter()
                    .any(|stmt| {
                        if let beskid_analysis::syntax::Statement::Expression(expr_stmt) =
                            &stmt.node
                        {
                            matches!(
                                expr_stmt.node.expression.node,
                                Expression::Spawn(_)
                            )
                        } else {
                            false
                        }
                    })
            } else {
                false
            }
        });
    assert!(has_spawn, "expected spawn expression in AST");
}
