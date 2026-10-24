//! Parse diagnostics: one test per known mistake (message, code, span, fix), escapes, and a corpus
//! check that no message leaks a grammar rule name or recovery wording.

use crate::analysis::diagnostics::{MietteReportError, SemanticDiagnostic};
use crate::analysis::{Applicability, Severity};
use crate::parser::Rule;

use super::parse_program_with_source_name_and_diagnostics;

/// The parse diagnostic for `source`, and whether the parser continued with a repaired tree.
fn diagnose(source: &str) -> (SemanticDiagnostic, bool) {
    match parse_program_with_source_name_and_diagnostics("t.bd", source) {
        Ok(parsed) => {
            assert!(parsed.recovered, "a repaired parse must say so");
            assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
            (parsed.diagnostics.into_iter().next().unwrap(), true)
        }
        Err(err) => {
            let diagnostic = err
                .downcast_ref::<MietteReportError>()
                .unwrap_or_else(|| panic!("structured diagnostic required, got {err:#}"))
                .diagnostic()
                .clone();
            (diagnostic, false)
        }
    }
}

fn spanned<'a>(source: &'a str, diagnostic: &SemanticDiagnostic) -> &'a str {
    let (start, end) = diagnostic.range();
    &source[start..end]
}

fn apply_fix(source: &str, diagnostic: &SemanticDiagnostic) -> String {
    let fix = diagnostic.fixes.first().expect("a fix");
    assert_eq!(fix.applicability, Applicability::MachineApplicable, "{}", fix.title);
    let mut edits = fix.edits.clone();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range().0));
    let mut fixed = source.to_string();
    for edit in edits {
        let (start, end) = edit.range();
        fixed.replace_range(start..end, &edit.replacement);
    }
    fixed
}

fn assert_shape(diagnostic: &SemanticDiagnostic, code: &str) {
    assert_eq!(diagnostic.code.as_deref(), Some(code), "{}", diagnostic.message);
    assert_eq!(diagnostic.severity, Severity::Error);
    assert!(diagnostic.help.is_some(), "one help line: {}", diagnostic.message);
    assert!(!diagnostic.message.contains("recovery"), "{}", diagnostic.message);
    assert!(!diagnostic.message.contains("parse error"), "{}", diagnostic.message);
}

#[test]
fn host_without_parameter_list_inserts_parentheses() {
    let source = "use Core.Env;\n\nhost MyHost {\n}\n";
    let (diagnostic, recovered) = diagnose(source);
    assert_shape(&diagnostic, "E0104");
    assert_eq!(diagnostic.message, "expected `(` while parsing host `MyHost`, found `{`");
    assert_eq!(spanned(source, &diagnostic), "{");
    assert!(diagnostic.help.as_deref().unwrap().contains("host MyHost() {"));
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].label, "host `MyHost` starts here");
    assert_eq!(&source[diagnostic.related[0].span.offset()..][..4], "host");
    assert_eq!(apply_fix(source, &diagnostic), "use Core.Env;\n\nhost MyHost() {\n}\n");
    assert!(recovered, "the fixed host parses, so the tree continues");
}

#[test]
fn host_with_base_and_no_parameter_list_inserts_parentheses() {
    let source = "host App : ConsoleHost {\n}\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0104");
    assert_eq!(spanned(source, &diagnostic), ":");
    assert_eq!(apply_fix(source, &diagnostic), "host App() : ConsoleHost {\n}\n");
}

#[test]
fn host_with_parameter_list_parses_cleanly() {
    let parsed = parse_program_with_source_name_and_diagnostics("t.bd", "host MyHost() {\n}\n").expect("parses");
    assert!(parsed.diagnostics.is_empty() && !parsed.recovered);
}

#[test]
fn missing_semicolon_inserts_after_the_statement() {
    let source = "i32 Main() {\n    i32 x = 5\n    return x;\n}\n";
    let (diagnostic, recovered) = diagnose(source);
    assert_shape(&diagnostic, "E0102");
    assert_eq!(diagnostic.message, "expected `;` while parsing binding `x`, found `return`");
    assert_eq!(spanned(source, &diagnostic), "return");
    assert_eq!(apply_fix(source, &diagnostic), "i32 Main() {\n    i32 x = 5;\n    return x;\n}\n");
    assert!(recovered);
}

#[test]
fn missing_semicolon_at_end_of_file() {
    let source = "use Lib.A";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0102");
    assert_eq!(diagnostic.message, "expected `;` while parsing `use` declaration, found end of file");
    assert_eq!(apply_fix(source, &diagnostic), "use Lib.A;");
}

#[test]
fn unclosed_parenthesis_inserts_the_closer() {
    let source = "i32 Main() {\n    i32 x = Foo(1, 2;\n    return x;\n}\n";
    let (diagnostic, recovered) = diagnose(source);
    assert_shape(&diagnostic, "E0103");
    assert_eq!(diagnostic.message, "expected `)` while parsing call to `Foo`, found `;`");
    assert_eq!(spanned(source, &diagnostic), ";");
    assert_eq!(apply_fix(source, &diagnostic), "i32 Main() {\n    i32 x = Foo(1, 2);\n    return x;\n}\n");
    assert!(recovered);
}

#[test]
fn unclosed_brace_at_end_of_file_inserts_the_closer() {
    let source = "i32 Main() {\n    return 0;\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0103");
    assert_eq!(diagnostic.message, "expected `}` while parsing function `Main`, found end of file");
    assert!(diagnostic.related.iter().any(|related| related.label == "unclosed `{`"));
    assert_eq!(apply_fix(source, &diagnostic), "i32 Main() {\n    return 0;\n}\n");
}

#[test]
fn increment_operator_becomes_compound_assignment() {
    let source = "unit Main() {\n    i32 i = 0;\n    i++;\n}\n";
    let (diagnostic, recovered) = diagnose(source);
    assert_shape(&diagnostic, "E0105");
    assert_eq!(diagnostic.message, "`++` is not a Beskid operator");
    assert_eq!(spanned(source, &diagnostic), "++");
    assert_eq!(diagnostic.help.as_deref(), Some("write `i += 1`"));
    assert_eq!(apply_fix(source, &diagnostic), "unit Main() {\n    i32 i = 0;\n    i += 1;\n}\n");
    assert!(recovered);
}

#[test]
fn decrement_operator_becomes_compound_assignment() {
    let source = "unit Main() {\n    i32 i = 3;\n    i--;\n}\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0105");
    assert_eq!(spanned(source, &diagnostic), "--");
    assert_eq!(apply_fix(source, &diagnostic), "unit Main() {\n    i32 i = 3;\n    i -= 1;\n}\n");
}

#[test]
fn rust_typed_let_becomes_type_first_binding() {
    let source = "i32 Main() {\n    let x: i32 = 5;\n    return x;\n}\n";
    let (diagnostic, recovered) = diagnose(source);
    assert_shape(&diagnostic, "E0106");
    assert_eq!(diagnostic.message, "expected `=` while parsing `let` binding `x`, found `:`");
    assert_eq!(spanned(source, &diagnostic), ":");
    assert_eq!(diagnostic.related[0].label, "`let` binding `x` starts here");
    assert_eq!(apply_fix(source, &diagnostic), "i32 Main() {\n    i32 x = 5;\n    return x;\n}\n");
    assert!(recovered);
}

#[test]
fn rust_typed_mutable_let_keeps_mut() {
    let source = "i32 Main() {\n    let mut total: i64[] = Make();\n    return 0;\n}\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0106");
    assert_eq!(apply_fix(source, &diagnostic), "i32 Main() {\n    mut i64[] total = Make();\n    return 0;\n}\n");
}

#[test]
fn reserved_word_as_binding_name_asks_for_another_name() {
    for source in ["unit Main() {\n    let test = 1;\n}\n", "unit Main() {\n    i32 test = 1;\n}\n"] {
        let (diagnostic, _) = diagnose(source);
        assert_shape(&diagnostic, "E0107");
        assert_eq!(diagnostic.message, "`test` is a reserved word; choose another name");
        assert_eq!(spanned(source, &diagnostic), "test");
        assert!(diagnostic.fixes.is_empty(), "renaming has no single meaning");
    }
}

#[test]
fn block_match_arm_value_is_explained() {
    let source = "i32 F(i32 x) {\n    return match x { 1 => { 5 }, _ => 0 };\n}\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0108");
    assert_eq!(diagnostic.message, "match arms cannot be blocks yet; call a function or use `if`");
    let (start, _) = diagnostic.range();
    assert_eq!(start, source.find("{ 5").unwrap());
    assert!(diagnostic.fixes.is_empty());
}

#[test]
fn foreign_function_syntax_gets_a_shape_help() {
    let source = "fn Add(a: i32) -> i32 {\n    return a;\n}\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0101");
    assert!(diagnostic.message.starts_with("expected "), "{}", diagnostic.message);
    assert!(diagnostic.message.contains(" while parsing "), "{}", diagnostic.message);
    assert!(diagnostic.fixes.is_empty(), "no single-meaning repair: {:?}", diagnostic.fixes);
}

#[test]
fn unknown_string_escape_has_its_own_code_and_span() {
    let source = "unit Main() {\n    string s = \"a\\qb\";\n}\n";
    let (diagnostic, recovered) = diagnose(source);
    assert!(!recovered);
    assert_shape(&diagnostic, "E0109");
    assert_eq!(diagnostic.message, "unknown escape `\\q` in string");
    assert_eq!(spanned(source, &diagnostic), "\\q");
}

#[test]
fn invalid_unicode_escape_has_its_own_code_and_span() {
    let source = "unit Main() {\n    string s = \"\\u{D800}\";\n}\n";
    let (diagnostic, _) = diagnose(source);
    assert_shape(&diagnostic, "E0110");
    assert_eq!(spanned(source, &diagnostic), "\\u{D800}");
}

#[test]
fn defined_string_escapes_parse() {
    let source =
        "unit Main() {\n    string s = \"tab\\tline\\ncr\\rnul\\0quote\\\"slash\\\\dollar\\${ \\u{263A}\";\n}\n";
    let parsed = parse_program_with_source_name_and_diagnostics("t.bd", source).expect("escapes parse");
    assert!(parsed.diagnostics.is_empty() && !parsed.recovered);
}

/// Common mistakes from the 0.6.7 research corpus: every message is words, never rule names.
#[test]
fn corpus_messages_never_name_grammar_rules() {
    let corpus = [
        "i32 Main() {\n    let x: i32 = 5;\n    return x;\n}\n",
        "i32 Main() {\n    let x;\n}\n",
        "fn Add(a: i32, b: i32) -> i32 {\n    return a + b;\n}\n",
        "i32 Add(i32 a, i32 b) -> i32 {\n    return a + b;\n}\n",
        "i32 Main() {\n    i32 x = 5\n    return x;\n}\n",
        "i32 Main() {\n    i32 x = Foo(1, 2;\n    return x;\n}\n",
        "i32 A() {\n    return 1;\ni32 B() {\n    return 2;\n}\n",
        "i32 Main(i32 x) {\n    if x > 0 return 1;\n    return 0;\n}\n",
        "i32 Main(i32 x) {\n    if x > 0 { return 1; } elif x < 0 { return 2; }\n    return 0;\n}\n",
        "unit Main() {\n    for (i32 i = 0; i < 3; i++) { }\n}\n",
        "unit Main() {\n    switch (x) { case 1: return; }\n}\n",
        "host App {\n    registry { }\n}\n",
        "const Max: i32 = 10;\n",
        "i32 Main(i32 x) {\n    return x > 0 ? 1 : 2;\n}\n",
        "bool Main() {\n    return true and false;\n}\n",
        "i32 Main(i32 x) {\n    return match x { 1 -> 2, _ -> 3 };\n}\n",
        "unit Main() {\n    i32[] xs = {1, 2, 3};\n}\n",
        "unit Main() {\n    string s = 'hello';\n}\n",
        "unit Main() {\n    string u = $\"v={x}\";\n}\n",
        "type Point {\n    i32 X;\n    i32 Y;\n}\n",
        "type Point {\n    i32 X\n    i32 Y\n}\n",
        "struct Point {\n    i32 X,\n}\n",
        "enum E { A = 1 }\n",
        "import Core.Env;\n",
        "test \"adds\" {\n}\n",
        "# comment\n",
        "unit Main() {\n    Foo(1,);\n}\n",
    ];
    let rule_names: Vec<String> = Rule::all_rules()
        .iter()
        .map(|rule| format!("{rule:?}"))
        .filter(|name| name.len() >= 7 && name.chars().skip(1).any(char::is_uppercase) || name == "Program")
        .collect();
    for source in corpus {
        let (diagnostic, recovered) = diagnose(source);
        eprintln!(
            "--- {source:?}\n    [{}] {} | label: {} | help: {:?} | related: {:?} | fixes: {:?} | recovered: {recovered}",
            diagnostic.code.as_deref().unwrap_or("-"),
            diagnostic.message,
            diagnostic.label,
            diagnostic.help,
            diagnostic.related.iter().map(|related| related.label.as_str()).collect::<Vec<_>>(),
            diagnostic.fixes.iter().map(|fix| fix.title.as_str()).collect::<Vec<_>>(),
        );
        let code = diagnostic.code.as_deref().unwrap_or("");
        assert!(code.starts_with("E01"), "{source:?}: code {code}");
        for name in &rule_names {
            assert!(
                !diagnostic.message.contains(name.as_str()),
                "{source:?}: rule name {name} in {}",
                diagnostic.message
            );
            assert!(!diagnostic.label.contains(name.as_str()), "{source:?}: rule name {name} in {}", diagnostic.label);
        }
        for leak in ["recovery", "-->", " = expected", "parse error"] {
            assert!(!diagnostic.message.contains(leak), "{source:?}: {leak} in {}", diagnostic.message);
        }
    }
}

#[test]
fn every_compound_assignment_operator_parses_in_a_body() {
    use crate::syntax::AssignOp;
    for (operator, expected) in [
        ("+=", AssignOp::AddAssign),
        ("-=", AssignOp::SubAssign),
        ("*=", AssignOp::MulAssign),
        ("/=", AssignOp::DivAssign),
        ("%=", AssignOp::RemAssign),
    ] {
        let source = format!("unit Main() {{\n    i32 x = 6;\n    x {operator} 2;\n}}\n");
        let parsed = parse_program_with_source_name_and_diagnostics("t.bd", &source).expect("parses");
        assert!(parsed.diagnostics.is_empty() && !parsed.recovered, "{operator}: {:?}", parsed.diagnostics);
        let text = format!("{:?}", parsed.program.node);
        assert!(text.contains(&format!("{expected:?}")), "{operator}: no {expected:?} node");
    }
}
