//! The formatter never renames. Renaming a declaration without its uses produces a program that
//! no longer resolves, so `beskid fmt` keeps every identifier as written; the naming-style lint
//! and its quick fix are the way to rename.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::format::{format_generated_program, format_source};
use beskid_analysis::services::{parse_program, semantic_rule_diagnostics_for_program};

/// Snake-case names in every naming role, each used after its declaration.
const USED_SNAKE_CASE: &str = r#"type point_2d { i32 x_pos; i32 y_pos; }
i32 add_one(i32 the_value) { let my_value = the_value + 1; return my_value; }
i32 sum_x(point_2d p) { let shifted = (i32 by_amount) => by_amount + p.x_pos; return shifted(add_one(p.y_pos)); }
test adds_one_to_zero { let one_more = add_one(0); return; }
"#;

/// Identifier words in source order (the corpus has no strings or comments).
fn identifiers(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    out.retain(|word| !word.chars().next().is_some_and(|c| c.is_ascii_digit()));
    out
}

/// Diagnostics without positions (the layout moves them), as `(code, message)` pairs.
fn semantic_findings(source: &str) -> Vec<(Option<String>, String)> {
    let program = parse_program(source).expect("parse");
    let mut findings: Vec<(Option<String>, String)> =
        semantic_rule_diagnostics_for_program(&program.node, "<memory>", source, AnalysisOptions::default())
            .into_iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.message))
            .collect();
    findings.sort();
    findings
}

#[test]
fn format_keeps_used_snake_case_names_and_the_program_is_unchanged() {
    let formatted = format_source("<memory>", USED_SNAKE_CASE).expect("format");
    assert_eq!(identifiers(&formatted), identifiers(USED_SNAKE_CASE), "an identifier changed:\n{formatted}");
    for name in ["point_2d", "x_pos", "add_one", "the_value", "my_value", "sum_x", "by_amount", "adds_one_to_zero"] {
        assert!(formatted.contains(name), "`{name}` was renamed:\n{formatted}");
    }

    // The same syntax tree: printing both trees without their source text gives the same text.
    let before = format_generated_program(&parse_program(USED_SNAKE_CASE).expect("parse input")).expect("print");
    let after = format_generated_program(&parse_program(&formatted).expect("parse output")).expect("print");
    assert_eq!(before, after, "formatting changed the program");

    // And the same analysis: every name still resolves exactly as before.
    assert_eq!(semantic_findings(&formatted), semantic_findings(USED_SNAKE_CASE));
    assert_eq!(format_source("<memory>", &formatted).expect("format again"), formatted);
}

#[test]
fn format_keeps_use_paths_and_test_names_as_written() {
    let source = "use core.io;\ntest checks_values { return; }\n";
    let formatted = format_source("<memory>", source).expect("format");
    assert!(formatted.starts_with("use core.io;\n"), "{formatted}");
    assert!(formatted.contains("test checks_values\n"), "{formatted}");
}

#[test]
fn format_keeps_a_non_conforming_declaration_that_is_never_used() {
    let source = "pub type hub_register { bool is_tty; }\n";
    let formatted = format_source("<memory>", source).expect("format");
    assert_eq!(formatted, "pub type hub_register\n{\n    bool is_tty;\n}\n");
}
