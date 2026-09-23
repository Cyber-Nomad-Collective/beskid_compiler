use crate::ast::{Expr, GrammarRule, parse_grammar_rules};
use crate::emit::emit_combinator_module;
use crate::expr_parser::parse_expr;
use crate::rule_name::rule_name_to_callable;

#[test]
fn parses_simple_rule() {
    let src = r#"digit = { "0" | "1" }"#;
    let rules = parse_grammar_rules(src).unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "digit");
}

#[test]
fn emits_literal_parser() {
    let rules = vec![GrammarRule { name: "hi".to_string(), expression: "\"hello\"".to_string() }];
    let out = emit_combinator_module("test", &rules);
    assert!(out.contains("ParseHi"));
    assert!(out.contains("Parser.Literal"));
    assert!(out.contains("Parser.Result.TextParseResult"));
    assert!(out.contains("use Core.Text.Parser.Terms;"));
}

#[test]
fn parses_optional_and_group() {
    let expr = parse_expr(r#"( "a" | "b" )? "#).unwrap();
    assert!(matches!(expr, Expr::Opt(_)));
}

#[test]
fn parses_regex_grammar_rules() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corelib/packages/foundation/grammars/regex.pest"));
    let rules = parse_grammar_rules(src).expect("regex.pest should parse");
    assert!(rules.iter().any(|rule| rule.name == "pat"));
    assert!(rules.iter().any(|rule| rule.name == "lower_run"));
}

#[test]
fn rule_name_to_callable_maps_snake_case() {
    assert_eq!(rule_name_to_callable("lower_run"), "ParseLowerRun");
    assert_eq!(rule_name_to_callable("digit"), "ParseDigit");
    assert_eq!(rule_name_to_callable("pat_branch"), "ParsePatBranch");
}

#[test]
fn emits_choice_backtracking() {
    let rules = vec![GrammarRule { name: "pick".to_string(), expression: "\"a\" | \"b\"".to_string() }];
    let out = emit_combinator_module("test", &rules);
    assert!(out.contains("ChoiceFailed"));
    assert!(out.contains("if Parser.IsOk(choice0)"));
}

#[test]
fn nested_repeat_threads_the_enclosing_sequence_cursor() {
    let rules = vec![GrammarRule { name: "pat".to_string(), expression: r#""a" ~ ("|" ~ "b")*"#.to_string() }];
    let out = emit_combinator_module("test", &rules);

    assert!(out.contains("mut Cursor.TextCursor seqCur = c;"));
    assert!(out.contains("mut Cursor.TextCursor seq1ManyCur = seqCur;"));
    assert!(out.contains("mut Cursor.TextCursor seq1SeqCur = seq1ManyCur;"));
    assert!(!out.contains("Parser.Literal(c, \"|\", \"pat\")"), "nested repeat restarted at c:\n{out}");
}

#[test]
fn nested_optional_preserves_a_successful_cursor_advance() {
    let rules = vec![GrammarRule { name: "maybe".to_string(), expression: r#""a" ~ "b"?"#.to_string() }];
    let out = emit_combinator_module("test", &rules);

    assert!(out.contains("mut Parser.Result.TextParseResult<string> seq1 = Parser.Pure(\"\", seqCur);"));
    assert!(out.contains("seq1 = Parser.Literal(seqCur, \"b\""));
    assert!(out.contains("if !Parser.IsOk(seq1)"));
    assert!(out.contains("seq1 = Parser.Pure(\"\", seqCur);"));
    assert!(!out.contains("seq1 = {"), "optional cursor result was discarded:\n{out}");
}

#[test]
fn negative_lookahead_is_zero_width_and_composable() {
    let rules = vec![GrammarRule { name: "class_char".to_string(), expression: r#"!("]") ~ ANY"#.to_string() }];
    let out = emit_combinator_module("test", &rules);

    assert!(out.contains("seq0 = Parser.Literal(seqCur, \"]\""));
    assert!(out.contains("seq0 = Parser.Pure(\"\", seqCur);"));
    assert!(out.contains("seqCur = Parser.RestOnOk(seq0, seqCur);"));
}

#[test]
fn generated_literals_escape_only_beskid_string_syntax() {
    let rules = vec![
        GrammarRule { name: "anchor".to_string(), expression: r#""$""#.to_string() },
        GrammarRule { name: "interpolation_text".to_string(), expression: r#""${""#.to_string() },
    ];
    let out = emit_combinator_module("test", &rules);

    assert!(out.contains(r#"Parser.Literal(c, "$", "anchor")"#));
    assert!(out.contains(r#"Parser.Literal(c, "\${", "interpolation_text")"#));
    assert!(!out.contains("unrepresentable literal"));
}
