#[cfg(test)]
mod tests {
    use crate::services::parse_recovery::collect_repair_candidates;
    use crate::parser::{BeskidParser, Rule};
    use pest::Parser;

    #[test]
    fn recovers_if_expression_with_missing_body() {
        let source = "if true";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();
        assert!(
            recovered_sources.iter().any(|candidate| candidate == "if true { }"),
            "expected control-flow body insertion for incomplete `if`"
        );
    }

    #[test]
    fn recovers_match_expression_without_body() {
        let source = "match x";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate == "match x { }"),
            "expected control-flow body insertion for incomplete `match`"
        );
    }

    #[test]
    fn recovers_unclosed_parens_with_stacked_suffixes() {
        let source = "fn x(a: i32, b: word";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate.contains(")")),
            "expected delimiter close insertion for unbalanced paren"
        );
    }

    #[test]
    fn recovers_let_binding_without_semicolon() {
        let source = "let value: Nat = 1";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate == "let value: Nat = 1;"),
            "expected statement terminator insertion for incomplete let binding"
        );
    }

    #[test]
    fn recovers_control_flow_without_body() {
        let source = "for i in values";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate == "for i in values { }"),
            "expected body placeholder insertion for incomplete for loop"
        );
    }

    #[test]
    fn recovers_missing_scope_body_block() {
        let source = "scope Scope() {\n";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed scope body");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate.starts_with("scope Scope() {") && candidate.ends_with('}')),
            "expected scope-body close insertion for incomplete scope block"
        );
    }

    #[test]
    fn recovers_missing_semicolon_before_next_statement() {
        let source = "let value: Nat = 1\nlet other: Nat = 2";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate == "let value: Nat = 1;\nlet other: Nat = 2"),
            "expected sync-driven semicolon insertion at statement boundary"
        );
    }

    #[test]
    fn recovers_missing_semicolon_before_item_keyword() {
        let source = "let value: Nat = 1\nhost App() {}";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate == "let value: Nat = 1;\nhost App() {}"),
            "expected sync boundary repair before host item keyword"
        );
    }

    #[test]
    fn recovers_missing_semicolon_before_expression_starts() {
        let cases = vec![
            ("let value: Nat = 1\ntrue", "true"),
            ("let value: Nat = 1\nfalse", "false"),
            ("let value: Nat = 1\n42", "42"),
            ("let value: Nat = 1\n\"text\"", "\"text\""),
            ("let value: Nat = 1\n[1, 2]", "[1, 2]"),
            ("let value: Nat = 1\n{ x: 1 }", "{ x: 1 }"),
            ("let value: Nat = 1\n(x, y)", "(x, y)"),
            ("let value: Nat = 1\n< T >", "< T >"),
            ("let value: Nat = 1\nFoo::Bar", "Foo::Bar"),
            ("let value: Nat = 1\nfoo()", "foo()"),
            ("let value: Nat = 1\nglobal::VALUE", "global::VALUE"),
            ("let value: Nat = 1\nparent::VALUE", "parent::VALUE"),
            ("let value: Nat = 1\n$x", "$x"),
            ("let value: Nat = 1\n!true", "!true"),
            ("let value: Nat = 1\n-1", "-1"),
            ("let value: Nat = 1\ncode```txt\n```", "code```txt\n```"),
        ];

        for (source, expected_tail) in cases {
            let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
            let recovered: Vec<_> = collect_repair_candidates("<repl>", source, &parse_error)
                .into_iter()
                .map(|(text, _)| text)
                .collect();

            assert!(
                recovered
                    .iter()
                    .any(|candidate| candidate == &format!("let value: Nat = 1;\n{expected_tail}")),
                "expected semicolon insertion before expression statement start `{expected_tail}`; candidates={recovered:?}"
            );
        }
    }

    #[test]
    fn recovers_missing_semicolon_before_multiple_syntax_starts() {
        let mut cases = vec![
            ("attribute [opt]", "attribute [opt]"),
            ("pub mod M {}", "pub mod M {}"),
            ("pub use Util", "pub use Util"),
            ("pub const VALUE = 1", "pub const VALUE = 1"),
            ("const VALUE = 1", "const VALUE = 1"),
            ("if true {}", "if true {}"),
            ("while cond {}", "while cond {}"),
            ("for i in items {}", "for i in items {}"),
            ("with App() {}", "with App() {}"),
            ("launch test() {}", "launch test() {}"),
            ("await task()", "await task()"),
            ("async task()", "async task()"),
            ("async task(1, 2)", "async task(1, 2)"),
            ("await task(1)", "await task(1)"),
            ("code ```txt\n```", "code ```txt\n```"),
            ("range(0, 1)", "range(0, 1)"),
            ("return 1", "return 1"),
            ("break", "break"),
            ("continue", "continue"),
            ("let x = 2", "let x = 2"),
            ("match x { _ => 1 }", "match x { _ => 1 }"),
            ("spawn run()", "spawn run()"),
            ("spawn task(1)", "spawn task(1)"),
            ("type T {}", "type T {}"),
            ("enum E {}", "enum E {}"),
            ("contract C {}", "contract C {}"),
            ("macro m() {}", "macro m() {}"),
            ("mod M {}", "mod M {}"),
            ("mod M", "mod M"),
            ("registry { transient Item }", "registry { transient Item }"),
            ("scope S()", "scope S()"),
            ("scope run() {}", "scope run() {}"),
            ("registry {}", "registry {}"),
            ("test t {}", "test t {}"),
            ("single Foo", "single Foo"),
            ("transient Bar", "transient Bar"),
            ("use Util", "use Util"),
            ("inject global::Value", "inject global::Value"),
            ("code ```txt\n```", "code ```txt\n```"),
            ("range(0, 1)", "range(0, 1)"),
            ("meta {}", "meta {}"),
            ("skip {}", "skip {}"),
            ("event Foo()", "event Foo()"),
            ("async task()", "async task()"),
            ("await task()", "await task()"),
            ("in values {}", "in values {}"),
            ("as Alias", "as Alias"),
            ("scope run() {}", "scope run() {}"),
            ("host App() {}", "host App() {}"),
            ("mut i32 x = 1", "mut i32 x = 1"),
            ("with App()", "with App()"),
            ("launch task()", "launch task()"),
            ("if true { true }", "if true { true }"),
            ("match x { _ => 1, _ => 2 }", "match x { _ => 1, _ => 2 }"),
        ];

        let bad_prefix = "let value: Nat = 1\n";
        for (tail, expected_tail) in cases.drain(..) {
            let source = format!("{bad_prefix}{tail}");
            let parse_error = BeskidParser::parse(Rule::Program, &source).expect_err("unexpectedly parsed malformed source");

        let candidates = collect_repair_candidates("<repl>", &source, &parse_error);
        let recovered_sources: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();
        let expected = format!("let value: Nat = 1;\n{expected_tail}");

        assert!(
            recovered_sources.iter().any(|candidate| candidate == &expected),
            "expected sync/statement boundary repair before `{expected_tail}`"
        );
        }
    }

    #[test]
    fn recovers_without_root_item_fallback_insertion() {
        let source = "let value: Nat = 1\nin values {}";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");

        let recovered_sources: Vec<_> = collect_repair_candidates("<repl>", source, &parse_error)
            .into_iter()
            .map(|(text, _)| text)
            .collect();

        assert!(
            recovered_sources.iter().any(|candidate| candidate == "let value: Nat = 1;\nin values {}"),
            "expected statement boundary insertion before `in`"
        );
        assert!(
            !recovered_sources.iter().any(|candidate| candidate == "let value = 0"),
            "root-level item-list fallback insertion should not be emitted in this case"
        );
    }

    #[test]
    fn prints_repair_samples_for_single_token_replacement() {
        let cases = vec![
            "fn f(x: i32, )",
            "if x => 1",
            "let value: Nat = 1 +",
            "range(0,)",
        ];

        for source in cases {
            let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
            let _ = collect_repair_candidates("<repl>", source, &parse_error);
        }
    }

    #[test]
    fn recovers_expression_with_missing_member_access_field() {
        let source = "let value = receiver.";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered.iter().any(|candidate| candidate.contains("receiver.field")),
            "expected member-access placeholder recovery"
        );
    }

    #[test]
    fn recovers_expression_with_missing_index_element() {
        let source = "let value = collection[";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered.iter().any(|candidate| candidate.contains("collection[0]")),
            "expected index-expression placeholder recovery"
        );
    }

    #[test]
    fn recovers_expression_with_trailing_operator() {
        let source = "let value = 1 +";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered.iter().any(|candidate| candidate.contains("1 +")),
            "expected trailing-operator placeholder recovery"
        );
    }

    #[test]
    fn recovers_trailing_comma_in_function_call_and_range_argument_lists() {
        let range_case = "fn f(x: i32, )";
        let range_error = BeskidParser::parse(Rule::Program, range_case).expect_err("unexpectedly parsed malformed source");
        let range_candidates = collect_repair_candidates("<repl>", range_case, &range_error);
        let range_recovered: Vec<_> = range_candidates.into_iter().map(|(text, _)| text).collect();

        let range_fixed = range_recovered
            .iter()
            .any(|candidate| matches!(candidate.as_str(), "fn f(x: i32, )" | "fn f(x: i32 )" | "fn f(x: i32)"));

        let call_case = "range(0,)";
        let call_error = BeskidParser::parse(Rule::Program, call_case).expect_err("unexpectedly parsed malformed source");
        let call_candidates = collect_repair_candidates("<repl>", call_case, &call_error);
        let call_recovered: Vec<_> = call_candidates.into_iter().map(|(text, _)| text).collect();
        let call_fixed = call_recovered
            .iter()
            .any(|candidate| matches!(candidate.as_str(), "range(0,0)" | "range(0)"));

        let tuple_case = "let x = (1,";
        let tuple_error = BeskidParser::parse(Rule::Program, tuple_case).expect_err("unexpectedly parsed malformed source");
        let tuple_candidates = collect_repair_candidates("<repl>", tuple_case, &tuple_error);
        let tuple_recovered: Vec<_> = tuple_candidates.into_iter().map(|(text, _)| text).collect();
        let tuple_fixed = tuple_recovered
            .iter()
            .any(|candidate| matches!(candidate.as_str(), "let x = (1,0)" | "let x = (1,)" | "let x = (1)"));
        assert!(
            range_fixed,
            "expected trailing-comma recovery for function signature lists"
        );
        assert!(
            call_fixed,
            "expected placeholder expression insertion to fix trailing comma in call arguments"
        );
        assert!(tuple_fixed, "expected trailing-comma recovery for parenthesized tuple-like lists");
    }

    #[test]
    fn recovers_trailing_comma_in_expression_array_lists() {
        let array_case = "let x = [1,";
        let array_error = BeskidParser::parse(Rule::Program, array_case).expect_err("unexpectedly parsed malformed source");
        let array_candidates = collect_repair_candidates("<repl>", array_case, &array_error);
        let array_recovered: Vec<_> = array_candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            array_recovered.iter().any(|candidate| candidate == "let x = [1,]" || candidate == "let x = [1]"),
            "expected trailing-comma deletion to recover incomplete array literals"
        );
    }

    #[test]
    fn recovers_from_repeated_tokens_across_list_like_and_call_syntax() {
        let cases = vec![
            ("let value = f(1,,2);", "call-argument list"),
            ("let value = [1,,2];", "array literal list"),
            ("let value = (1,,2);", "tuple-like parenthesized list"),
            ("let value: Foo<a,,b> = 0;", "generic type argument list"),
            ("let value = Foo { a: 1,, b: 2 };", "struct field list"),
        ];

        for (source, label) in cases {
            let parse_error = BeskidParser::parse(Rule::Program, source).expect_err(&format!("unexpectedly parsed malformed source: {label}"));
            let candidates = collect_repair_candidates("<repl>", source, &parse_error);
            let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

            assert!(
                recovered.iter().any(|candidate| {
                    !candidate.contains(",,") && (candidate.contains(",") || candidate.contains("}"))
                }),
                "expected duplicate-token recovery for {label}; candidates: {:#?}",
                recovered
            );
        }
    }

    #[test]
    fn recovers_trailing_comma_in_angle_type_lists() {
        let source = "let value: Foo<T,";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered.iter().any(|candidate| candidate == "let value: Foo<T>"),
            "expected generic close insertion for incomplete type-angle list"
        );
        assert!(
            recovered.iter().any(|candidate| candidate.starts_with("let value: Foo<T,T")),
            "expected angle-list trailing-comma recovery for generic/type argument lists"
        );
    }

    #[test]
    fn recovers_trailing_comma_in_struct_literal_fields() {
        let source = "let value = Foo { a: 1,";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered
                .iter()
                .any(|candidate| {
                    matches!(
                        candidate.as_str(),
                        "let value = Foo { a: 1,}" | "let value = Foo { a: 1 }" | "let value = Foo { a: 1, field: 0 }"
                    )
                }),
            "expected trailing-comma deletion for incomplete struct field list"
        );
    }

    #[test]
    fn recovers_trailing_comma_in_type_field_lists() {
        let source = "type User { id: word,";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered.iter().any(|candidate| candidate.contains("field: word")),
            "expected type field list repair for trailing comma in TypeFieldList"
        );
    }

    #[test]
    fn recovers_trailing_comma_in_enum_variant_lists() {
        let source = "enum Event { Started, Running,";
        let parse_error = BeskidParser::parse(Rule::Program, source).expect_err("unexpectedly parsed malformed source");
        let candidates = collect_repair_candidates("<repl>", source, &parse_error);
        let recovered: Vec<_> = candidates.into_iter().map(|(text, _)| text).collect();

        assert!(
            recovered.iter().any(|candidate| candidate.contains("Value")),
            "expected enum variant placeholder repair for trailing comma in EnumVariantList"
        );
    }
}
