//! Formatter round-trip tests for language macro AST nodes.

use beskid_analysis::format::format_program;
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::expressions::{Expression, MacroInvocation};
use beskid_analysis::syntax::items::{MacroDefinition, MacroFragmentKind, Node};
use beskid_analysis::syntax::{Program, Spanned};

fn parse_format_reparse(source: &str) -> Spanned<Program> {
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program).expect("format");
    parse_program(&formatted).expect("re-parse formatted")
}

fn first_macro_definition(program: &Program) -> Spanned<MacroDefinition> {
    program
        .items
        .iter()
        .find_map(|item| match &item.node {
            Node::MacroDefinition(def) => Some(def.clone()),
            _ => None,
        })
        .expect("macro definition item")
}

fn first_macro_invocation_in_program(program: &Program) -> Spanned<MacroInvocation> {
    for item in &program.items {
        if let Node::Function(f) = &item.node {
            for stmt in &f.node.body.node.statements {
                if let Some(inv) = find_invocation_in_statement(&stmt.node) {
                    return inv;
                }
            }
        }
    }
    panic!("macro invocation not found");
}

fn find_invocation_in_statement(stmt: &beskid_analysis::syntax::Statement) -> Option<Spanned<MacroInvocation>> {
    use beskid_analysis::syntax::Statement;
    let expr = match stmt {
        Statement::Expression(es) => Some(&es.node.expression),
        Statement::Let(ls) => Some(&ls.node.value),
        _ => None,
    }?;
    find_invocation_in_expression(expr)
}

fn find_invocation_in_expression(expr: &Spanned<Expression>) -> Option<Spanned<MacroInvocation>> {
    match &expr.node {
        Expression::MacroInvocation(inv) => Some(inv.clone()),
        _ => None,
    }
}

#[test]
fn macro_definition_roundtrip_preserves_name_and_parameters() {
    let source = r#"
macro twice (expression value, block body) {
    $value;
    $body;
}

unit Main() {
    twice!(1) {
        return;
    };
    return;
}
"#;
    let before = parse_program(source).expect("parse");
    let def_before = first_macro_definition(&before.node);
    let after = parse_format_reparse(source);
    let def_after = first_macro_definition(&after.node);

    assert_eq!(def_before.node.name.node.name, def_after.node.name.node.name);
    assert_eq!(
        def_before.node.parameters.len(),
        def_after.node.parameters.len(),
        "parameter count must survive format round-trip"
    );
    let kinds_before: Vec<MacroFragmentKind> = def_before.node.parameters.iter().map(|p| p.node.kind.node).collect();
    let kinds_after: Vec<MacroFragmentKind> = def_after.node.parameters.iter().map(|p| p.node.kind.node).collect();
    assert_eq!(kinds_before, kinds_after);
}

#[test]
fn macro_definition_roundtrip_preserves_macro_name() {
    let source = "macro id (expression x) { $x; }\nunit Main() { return; }\n";
    let before = parse_program(source).expect("parse");
    let def_before = first_macro_definition(&before.node);
    let after = parse_format_reparse(source);
    let def_after = first_macro_definition(&after.node);
    assert_eq!(def_before.node.name.node.name, def_after.node.name.node.name);
}

#[test]
fn macro_invocation_roundtrip_preserves_arguments_and_block() {
    let source = r#"
macro m (expression a, block b) { $a; $b; }
unit Main() {
    m!(1, 2) {
        return;
    };
    return;
}
"#;
    let before = parse_program(source).expect("parse");
    let inv_before = first_macro_invocation_in_program(&before.node);
    let after = parse_format_reparse(source);
    let inv_after = first_macro_invocation_in_program(&after.node);

    assert_eq!(inv_before.node.arguments.len(), inv_after.node.arguments.len());
    assert_eq!(inv_before.node.block.is_some(), inv_after.node.block.is_some());
    assert_eq!(inv_before.node.name.node.name, inv_after.node.name.node.name);
}

#[test]
fn macro_metavariable_roundtrip_preserves_parameter_names() {
    let source = "macro m (expression x) { $x; }\nunit Main() { return; }\n";
    let program = parse_program(source).expect("parse");
    let formatted = format_program(&program).expect("format");
    assert!(formatted.contains("$x"), "formatted macro body should retain $x metavariable, got:\n{formatted}");
}
