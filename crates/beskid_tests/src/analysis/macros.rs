//! Language macro expansion (`macro.expand`) conformance.

use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::expressions::Expression;
use beskid_analysis::syntax::items::{MacroFragmentKind, Node};
use beskid_pipeline::phases::{FULL_BUILD_PHASE_ORDER, MACRO_EXPAND, PARSE};

use super::macros_support::{
    assert_no_macro_invocations_in_block, assert_no_macro_invocations_in_expr,
    block_contains_macro_invocation_named, count_macro_invocations,
    expression_contains_binary_with_literal, find_function_body, parse_expand,
    parse_expand_with_depth,
};

#[test]
fn full_build_phase_order_places_macro_expand_after_parse() {
    let parse = FULL_BUILD_PHASE_ORDER
        .iter()
        .position(|p| *p == PARSE)
        .expect("parse");
    let expand = FULL_BUILD_PHASE_ORDER
        .iter()
        .position(|p| *p == MACRO_EXPAND)
        .expect("macro.expand");
    let mod_load = FULL_BUILD_PHASE_ORDER
        .iter()
        .position(|p| *p == "mod.load")
        .expect("mod.load");
    assert!(parse < expand);
    assert!(expand < mod_load);
}

#[test]
fn macro_expansion_substitutes_expression_body() {
    let source = r#"
macro twice (expression value) {
    $value + $value;
}

unit Main() {
    let x = twice!(1);
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    let let_stmt = body.node.statements.first().expect("let statement");
    let expr = match &let_stmt.node {
        beskid_analysis::syntax::Statement::Let(ls) => &ls.node.value,
        other => panic!("expected let, got {other:?}"),
    };
    assert!(
        matches!(expr.node, Expression::Binary(_)),
        "expected expanded addition, got {:?}",
        expr.node
    );
    assert_no_macro_invocations_in_expr(&expr.node);
}

#[test]
fn block_fragment_braced_invocation_splices_into_caller() {
    let source = r#"
macro wrap (block body) {
    $body;
}

unit Main() {
    wrap!() {
        let x = 1;
    };
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    assert!(
        body.node.statements.len() >= 2,
        "expected spliced let plus return, got {} statements",
        body.node.statements.len()
    );
    let first = &body.node.statements[0].node;
    assert!(
        matches!(first, beskid_analysis::syntax::Statement::Let(_)),
        "expected spliced let statement, got {first:?}"
    );
    assert_no_macro_invocations_in_block(&body.node);
}

#[test]
fn nested_macro_expands_to_fixed_point_in_one_pass() {
    let source = r#"
macro inner (expression value) {
    $value + $value;
}

macro outer () {
    inner!(1);
}

unit Main() {
    let x = outer!();
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    let let_stmt = body.node.statements.first().expect("let");
    let expr = match &let_stmt.node {
        beskid_analysis::syntax::Statement::Let(ls) => &ls.node.value,
        other => panic!("expected let, got {other:?}"),
    };
    assert!(
        expression_contains_binary_with_literal(&expr.node, 1),
        "expected expanded 1 + 1, got {:?}",
        expr.node
    );
    assert_no_macro_invocations_in_expr(&expr.node);
}

#[test]
fn expansion_depth_cap_emits_e1905() {
    let source = r#"
macro a (expression x) { b!($x); }
macro b (expression x) { a!($x); }

unit Main() {
    let x = a!(1);
    return;
}
"#;
    let program = parse_program_with_source_name("Main.bd", source).expect("parse");
    let outcome =
        beskid_analysis::macros::expand_program_with_diagnostics(program, 2, "Main.bd", source);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("E1905")),
        "expected E1905, got {:?}",
        outcome.diagnostics
    );
}

#[test]
fn expansion_depth_cap_leaves_invocations_when_exceeded() {
    let source = r#"
macro a (expression x) { b!($x); }
macro b (expression x) { a!($x); }

unit Main() {
    let x = a!(1);
    return;
}
"#;
    let expanded = parse_expand_with_depth(source, 2);
    assert!(
        count_macro_invocations(&expanded.node) > 0,
        "expected residual invocations after depth cap on ping-pong macros"
    );
}

#[test]
fn macro_in_if_body_expands() {
    let source = r#"
macro one (expression x) { $x; }

unit Main() {
    if true {
        let x = one!(1);
    }
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    let if_stmt = body.node.statements.first().expect("if statement");
    let then_block = match &if_stmt.node {
        beskid_analysis::syntax::Statement::If(i) => &i.node.then_block,
        other => panic!("expected if, got {other:?}"),
    };
    assert_no_macro_invocations_in_block(&then_block.node);
}

#[test]
fn macro_in_inline_module_registry_and_expansion() {
    let source = r#"
mod Inner {
    macro double (expression value) {
        $value + $value;
    }

    unit helper() {
        let x = double!(2);
        return;
    }
}

unit Main() {
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "helper");
    let let_stmt = body.node.statements.first().expect("let");
    let expr = match &let_stmt.node {
        beskid_analysis::syntax::Statement::Let(ls) => &ls.node.value,
        other => panic!("expected let, got {other:?}"),
    };
    assert!(
        expression_contains_binary_with_literal(&expr.node, 2),
        "expected expanded 2 + 2, got {:?}",
        expr.node
    );
}

#[test]
fn duplicate_macro_name_last_definition_wins() {
    let source = r#"
macro pick (expression value) { $value; }

macro pick (expression value) {
    $value + $value;
}

unit Main() {
    let x = pick!(3);
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    let let_stmt = body.node.statements.first().expect("let");
    let expr = match &let_stmt.node {
        beskid_analysis::syntax::Statement::Let(ls) => &ls.node.value,
        other => panic!("expected let, got {other:?}"),
    };
    assert!(
        expression_contains_binary_with_literal(&expr.node, 3),
        "second macro definition should win (doubling), got {:?}",
        expr.node
    );
}

#[test]
fn arity_mismatch_emits_e1902() {
    let source = r#"
macro need_one (expression x) { $x; }

unit Main() {
    let x = need_one!();
    return;
}
"#;
    let program = parse_program_with_source_name("Main.bd", source).expect("parse");
    let outcome = beskid_analysis::macros::expand_program_with_diagnostics(
        program,
        beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        "Main.bd",
        source,
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("E1902")),
        "expected E1902, got {:?}",
        outcome.diagnostics
    );
}

#[test]
fn fragment_kind_mismatch_expression_param_block_actual_emits_e1903() {
    let source = r#"
macro need_literal (literal lit) { $lit; }

unit Main() {
    need_literal!(x);
    return;
}
"#;
    let program = parse_program_with_source_name("Main.bd", source).expect("parse");
    let outcome = beskid_analysis::macros::expand_program_with_diagnostics(
        program,
        beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
        "Main.bd",
        source,
    );
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("E1903")),
        "expected E1903, got {:?}",
        outcome.diagnostics
    );
}

#[test]
fn unknown_macro_invocation_remains_for_semantic_pass() {
    let source = "unit Main() { missing!(1); return; }\n";
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    let stmt = &body.node.statements[0];
    let expr = match &stmt.node {
        beskid_analysis::syntax::Statement::Expression(es) => &es.node.expression,
        other => panic!("expected expression stmt, got {other:?}"),
    };
    assert!(matches!(expr.node, Expression::MacroInvocation(_)));
}

#[test]
fn fragment_kind_mismatch_leaves_invocation_unexpanded() {
    let source = r#"
macro need_expr (expression x) { $x; }

unit Main() {
    need_expr!() {
        let a = 1;
    };
    return;
}
"#;
    let expanded = parse_expand(source);
    let body = find_function_body(&expanded.node, "main");
    assert!(
        block_contains_macro_invocation_named(&body.node, "need_expr"),
        "expression parameter with block-only actual should not expand yet"
    );
}

#[test]
fn corelib_compiler_sdk_identity_macro_expands() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/compiler-sdk/src/Beskid/Macros.bd");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let program = parse_program_with_source_name("Macros.bd", &source).expect("parse");
    let expanded = expand_program(program, DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let has_macro_def = expanded
        .node
        .items
        .iter()
        .any(|item| matches!(&item.node, Node::MacroDefinition(_)));
    assert!(has_macro_def, "macro definition should remain in the unit");
}

// --- 1B: per-fragment-kind matrix (table-driven) ---

struct FragmentKindCase {
    kind: &'static str,
    macro_def: &'static str,
    valid_invocation: &'static str,
    /// Invocation shape that does not match the parameter kind (expects E1903 or no expansion).
    invalid_invocation: &'static str,
    /// When true, expansion should remove all macro invocations from `main` after expand.
    expect_valid_expands: bool,
}

fn fragment_kind_cases() -> Vec<FragmentKindCase> {
    vec![
        FragmentKindCase {
            kind: "block",
            macro_def: "macro m (block b) { $b; }",
            valid_invocation: "m!() { let x = 1; };",
            invalid_invocation: "let x = m!(1);",
            expect_valid_expands: true,
        },
        FragmentKindCase {
            kind: "expression",
            macro_def: "macro m (expression e) { $e; }",
            valid_invocation: "let x = m!(1);",
            invalid_invocation: "m!() { return; };",
            expect_valid_expands: true,
        },
        FragmentKindCase {
            kind: "statement",
            macro_def: "macro m (statement s) { $s; }",
            valid_invocation: "m! { return; }",
            invalid_invocation: "let x = m!(1);",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "type",
            macro_def: "macro m (type t) { }",
            valid_invocation: "m!(i32);",
            invalid_invocation: "m!() { return; };",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "identifier",
            macro_def: "macro m (identifier id) { }",
            valid_invocation: "m!(x);",
            invalid_invocation: "m!(42);",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "literal",
            macro_def: "macro m (literal lit) { $lit; }",
            valid_invocation: "let x = m!(42);",
            invalid_invocation: "m!(x);",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "pattern",
            macro_def: "macro m (pattern p) { }",
            valid_invocation: "let v = match 0 { m!(_) => 1, };",
            invalid_invocation: "let x = m!(1);",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "path",
            macro_def: "macro m (path p) { }",
            valid_invocation: "m!(std);",
            invalid_invocation: "m!(42);",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "item",
            macro_def: "macro m (item it) { }",
            valid_invocation: "m!(type T { i32 x; });",
            invalid_invocation: "let x = m!(1);",
            expect_valid_expands: false,
        },
        FragmentKindCase {
            kind: "node",
            macro_def: "macro m (node n) { }",
            valid_invocation: "m!(return;);",
            invalid_invocation: "m!() { return; };",
            expect_valid_expands: false,
        },
    ]
}

#[test]
fn fragment_kind_matrix_valid_invocations_parse() {
    for case in fragment_kind_cases() {
        if matches!(case.kind, "statement" | "node" | "pattern" | "item") {
            continue;
        }
        let source = format!(
            "{}\nunit Main() {{\n    {}\n    return;\n}}\n",
            case.macro_def, case.valid_invocation
        );
        parse_program_with_source_name("Main.bd", &source)
            .unwrap_or_else(|e| panic!("parse valid `{}`: {e}", case.kind));
    }
}

#[test]
fn fragment_kind_matrix_invalid_invocations_parse() {
    for case in fragment_kind_cases() {
        let source = format!(
            "{}\nunit Main() {{\n    {}\n    return;\n}}\n",
            case.macro_def, case.invalid_invocation
        );
        parse_program_with_source_name("Main.bd", &source)
            .unwrap_or_else(|e| panic!("parse invalid `{}`: {e}", case.kind));
    }
}

#[test]
fn fragment_kind_matrix_valid_expansion_behavior() {
    for case in fragment_kind_cases() {
        if matches!(
            case.kind,
            "statement" | "node" | "pattern" | "type" | "identifier" | "literal" | "path" | "item"
        ) {
            continue;
        }
        let source = format!(
            "{}\nunit Main() {{\n    {}\n    return;\n}}\n",
            case.macro_def, case.valid_invocation
        );
        let expanded = parse_expand(&source);
        let body = find_function_body(&expanded.node, "main");
        let has_invocation = block_contains_macro_invocation_named(&body.node, "m");
        if case.expect_valid_expands {
            assert!(
                !has_invocation,
                "kind `{}` valid invocation should expand fully",
                case.kind
            );
        } else {
            assert!(
                has_invocation || count_macro_invocations(&expanded.node) > 0,
                "kind `{}` valid invocation should remain unexpanded until binding support lands",
                case.kind
            );
        }
    }
}

#[test]
fn fragment_kind_enum_matches_parser_keywords() {
    let kinds = [
        ("block", MacroFragmentKind::Block),
        ("expression", MacroFragmentKind::Expression),
        ("statement", MacroFragmentKind::Statement),
        ("type", MacroFragmentKind::Type),
        ("identifier", MacroFragmentKind::Identifier),
        ("literal", MacroFragmentKind::Literal),
        ("pattern", MacroFragmentKind::Pattern),
        ("path", MacroFragmentKind::Path),
        ("item", MacroFragmentKind::Item),
        ("node", MacroFragmentKind::Node),
    ];
    for (kw, expected) in kinds {
        assert_eq!(
            MacroFragmentKind::from_keyword(kw),
            Some(expected),
            "keyword `{kw}`"
        );
    }
}
