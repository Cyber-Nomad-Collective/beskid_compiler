//! Formatter (`Emit`) integration tests.

use beskid_analysis::format::Emitter;
use beskid_analysis::format::format_program;
use beskid_analysis::parsing::parsable::Parsable;
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax::items::impl_block::ImplBlock;
use beskid_analysis::syntax::{Block, Expression, Node, Program, Spanned, Statement};
use beskid_analysis::{BeskidParser, Rule};
use pest::Parser;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn format_program_is_idempotent() {
    let src = r#"use a.b;


pub i32 main() { return 42; }
"#;
    let p = parse_program(src).expect("parse");
    let once = format_program(&p).expect("format");
    let p2 = parse_program(&once).expect("re-parse formatted");
    let twice = format_program(&p2).expect("re-format");
    assert_eq!(once, twice, "formatter output must be stable");
}

#[test]
fn format_golden_use_and_function() {
    let src = "use a.b;\npub i32 main() { return 42; }\n";
    let p = parse_program(src).expect("parse");
    let out = format_program(&p).expect("format");
    let expected = concat!(
        "use a.b;\n",
        "\n",
        "pub i32 main()\n",
        "{\n",
        "    return 42;\n",
        "}\n",
    );
    assert_eq!(out, expected);
}

#[test]
fn format_if_while_use_parentheses_and_blank_line_before_let() {
    let src = r#"pub unit f() {
if cond { return; }
while cond2 { break; }
let x = 1;
}"#;
    let p = parse_program(src).expect("parse");
    let out = format_program(&p).expect("format");
    let expected = concat!(
        "pub unit f()\n",
        "{\n",
        "    if (cond)\n",
        "    {\n",
        "        return;\n",
        "    }\n",
        "    while (cond2)\n",
        "    {\n",
        "        break;\n",
        "    }\n",
        "\n",
        "    let x = 1;\n",
        "}\n",
    );
    assert_eq!(out, expected);
}

#[test]
fn format_if_condition_grouped_idempotent() {
    let src = r#"pub unit f() { if ((cond)) { return; } }"#;
    let p = parse_program(src).expect("parse");
    let once = format_program(&p).expect("format");
    let p2 = parse_program(&once).expect("re-parse");
    let twice = format_program(&p2).expect("re-format");
    assert_eq!(
        once, twice,
        "grouped if condition must not accumulate extra parens"
    );
    assert!(
        once.contains("if (") && once.contains("cond"),
        "expected if header with condition, got:\n{once}"
    );
}

#[test]
fn format_binary_ops_emit_canonical_operators() {
    // Note: `<=` / `>=` are not covered here inside `let` assignments because the current
    // concrete syntax tokenization can treat `=` in `<=` as starting a new construct; those
    // operators are still exercised via `parse_expression` tests elsewhere.
    let cases = [
        ("||", "true || false"),
        ("&&", "true && false"),
        ("===", "1 === 1"),
        ("!==", "1 !== 2"),
        ("==", "1 == 1"),
        ("!=", "1 != 2"),
        ("<", "1 < 2"),
        (">", "1 > 0"),
        ("+", "1 + 2"),
        ("-", "3 - 2"),
        ("*", "2 * 3"),
        ("/", "4 / 2"),
    ];
    for (op, expr_src) in cases {
        let src = format!("pub unit t() {{ let _ = {expr_src}; return; }}");
        let p = parse_program(&src).unwrap_or_else(|e| panic!("parse {op}: {e}"));
        let out = format_program(&p).unwrap_or_else(|e| panic!("format {op}: {e:?}"));
        assert!(
            out.contains(op),
            "formatted output should contain operator `{op}`; got:\n{out}"
        );
    }
}

#[test]
fn format_type_enum_contract_match() {
    let src = r#"use std.io;
pub type Point { i32 x, i32 y, }
pub enum E { A, B(i32 x,) }
pub contract C { i32 m(); Other }
pub unit demo() {
let v = match 0 { _ => 1, };
}"#;
    let p = parse_program(src).expect("parse");
    let out = format_program(&p).expect("format");
    let expected = concat!(
        "use std.io;\n",
        "\n",
        "pub type Point\n",
        "{\n",
        "    i32 x,\n",
        "\n",
        "    i32 y,\n",
        "}\n",
        "\n",
        "pub enum E\n",
        "{\n",
        "    A,\n",
        "\n",
        "    B(i32 x),\n",
        "}\n",
        "\n",
        "pub contract C\n",
        "{\n",
        "    i32 m();\n",
        "\n",
        "    Other;\n",
        "}\n",
        "\n",
        "pub unit demo()\n",
        "{\n",
        "    let v = match 0\n",
        "    {\n",
        "        _ => 1,\n",
        "    };\n",
        "}\n",
    );
    assert_eq!(out, expected);
}

#[test]
fn fixture_corpus_is_stable_and_parse_preserving() {
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/format");
    let paths = collect_input_fixtures(&fixture_root);
    assert!(!paths.is_empty(), "expected at least one format fixture");

    for path in &paths {
        let input = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let expected_path = path.with_file_name(
            path.file_name()
                .and_then(|n| n.to_str())
                .expect("fixture name")
                .replace(".input.bd", ".expected.bd"),
        );
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));

        let original = parse_program(&input)
            .unwrap_or_else(|e| panic!("fixture input parses {:?}: {e}", path));
        let formatted = format_program(&original)
            .unwrap_or_else(|e| panic!("fixture formats {:?}: {e:?}", path));
        if formatted != expected {
            panic!(
                "fixture golden mismatch for {:?}\n{}",
                path,
                first_mismatch_lines(&formatted, &expected)
            );
        }

        let reparsed = parse_program(&formatted)
            .unwrap_or_else(|e| panic!("formatted fixture reparses {:?}: {e}", path));
        let reformatted = format_program(&reparsed)
            .unwrap_or_else(|e| panic!("reformat formatted fixture {:?}: {e:?}", path));
        if reformatted != formatted {
            panic!(
                "fixture not idempotent for {:?}\n{}",
                path,
                first_mismatch_lines(&reformatted, &formatted)
            );
        }

        let lhs_kinds = top_level_kinds_relaxed(&original.node.items);
        let rhs_kinds = top_level_kinds_relaxed(&reparsed.node.items);
        assert_eq!(
            lhs_kinds, rhs_kinds,
            "top-level node sequence changed for {:?}",
            path
        );

        let c0 = program_statement_count(&original.node);
        let c1 = program_statement_count(&reparsed.node);
        assert_eq!(
            c0, c1,
            "statement count changed for {:?} (before {c0}, after {c1})",
            path
        );
    }
}

fn collect_input_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_input_fixtures_inner(root, &mut out);
    out.sort();
    out
}

fn collect_input_fixtures_inner(dir: &Path, out: &mut Vec<PathBuf>) {
    let read_dir = match fs::read_dir(dir) {
        Ok(d) => d,
        Err(_) => return,
    };
    for entry in read_dir.flatten() {
        let p = entry.path();
        let ty = entry.file_type().ok();
        if ty.map(|t| t.is_dir()).unwrap_or(false) {
            collect_input_fixtures_inner(&p, out);
            continue;
        }
        if p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".input.bd"))
        {
            out.push(p);
        }
    }
}

fn first_mismatch_lines(formatted: &str, expected: &str) -> String {
    let fa: Vec<&str> = formatted.lines().collect();
    let ea: Vec<&str> = expected.lines().collect();
    let max = fa.len().max(ea.len());
    for i in 0..max {
        let f = fa.get(i).copied().unwrap_or("<EOF>");
        let e = ea.get(i).copied().unwrap_or("<EOF>");
        if f != e {
            let line = i + 1;
            return format!(
                "first diff at 1-based line {line}:\n  formatted: {f:?}\n  expected:  {e:?}"
            );
        }
    }
    "(same lines but text differed — check trailing newline)".to_string()
}

fn top_level_kinds(items: &[beskid_analysis::syntax::Spanned<Node>]) -> Vec<&'static str> {
    items
        .iter()
        .map(|item| match item.node {
            Node::Function(_) => "function",
            Node::Method(_) => "method",
            Node::TypeDefinition(_) => "type",
            Node::EnumDefinition(_) => "enum",
            Node::ContractDefinition(_) => "contract",
            Node::TestDefinition(_) => "test",
            Node::AttributeDeclaration(_) => "attribute",
            Node::ModuleDeclaration(_) => "module_decl",
            Node::InlineModule(_) => "inline_module",
            Node::UseDeclaration(_) => "use",
        })
        .collect()
}

/// `impl` blocks parse as `Node::Method` items; the formatter may emit them as `function` items.
/// Treat both as the same bucket for parse-preservation checks.
fn top_level_kinds_relaxed(items: &[beskid_analysis::syntax::Spanned<Node>]) -> Vec<&'static str> {
    top_level_kinds(items)
        .into_iter()
        .map(|k| if k == "method" { "function" } else { k })
        .collect()
}

/// Counts `Statement` nodes in function / method / test bodies and nested control-flow blocks.
fn program_statement_count(program: &Program) -> usize {
    program
        .items
        .iter()
        .map(|item| count_statements_in_node(&item.node))
        .sum()
}

fn count_statements_in_node(node: &Node) -> usize {
    match node {
        Node::Function(f) => count_block_statements(&f.node.body.node),
        Node::Method(m) => count_block_statements(&m.node.body.node),
        Node::InlineModule(im) => im
            .node
            .items
            .iter()
            .map(|i| count_statements_in_node(&i.node))
            .sum(),
        Node::TestDefinition(t) => count_statement_slice(&t.node.statements),
        _ => 0,
    }
}

fn count_block_statements(block: &Block) -> usize {
    count_statement_slice(&block.statements)
}

fn count_statement_slice(stmts: &[Spanned<Statement>]) -> usize {
    stmts
        .iter()
        .map(|s| count_statement(&s.node))
        .sum::<usize>()
}

fn count_statement(stmt: &Statement) -> usize {
    1 + match stmt {
        Statement::If(i) => {
            count_block_statements(&i.node.then_block.node)
                + i.node
                    .else_block
                    .as_ref()
                    .map(|b| count_block_statements(&b.node))
                    .unwrap_or(0)
        }
        Statement::While(w) => count_block_statements(&w.node.body.node),
        Statement::For(f) => count_block_statements(&f.node.body.node),
        Statement::Let(l) => count_expr_blocks(&l.node.value.node),
        Statement::Return(r) => r
            .node
            .value
            .as_ref()
            .map(|e| count_expr_blocks(&e.node))
            .unwrap_or(0),
        Statement::Expression(e) => count_expr_blocks(&e.node.expression.node),
        Statement::Break(_) | Statement::Continue(_) => 0,
    }
}

fn count_expr_blocks(expr: &Expression) -> usize {
    match expr {
        Expression::Block(b) => count_block_statements(&b.node.block.node),
        Expression::Match(m) => m
            .node
            .arms
            .iter()
            .map(|a| count_expr_blocks(&a.node.value.node))
            .sum(),
        Expression::Lambda(l) => match &l.node.body.node {
            Expression::Block(b) => count_block_statements(&b.node.block.node),
            other => count_expr_blocks(other),
        },
        _ => 0,
    }
}

#[test]
fn impl_block_emitter_works_for_structural_ast() {
    let source = r#"impl Number {
pub i32 abs(i32 value) { return value; }
}"#;
    let mut pairs = BeskidParser::parse(Rule::ImplBlock, source).expect("parse impl block");
    let pair = pairs.next().expect("impl block pair");
    let parsed = ImplBlock::parse(pair).expect("impl block ast");
    let text = Emitter::new().write(&parsed.node).expect("emit impl block");
    assert!(text.contains("impl Number"));
    assert!(text.contains("pub i32 abs(i32 value)"));
}
