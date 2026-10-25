//! Formatter behaviour: comments survive, parse errors fail closed, blank-line and brace style.

use super::{FormatError, format_source};

fn fmt(source: &str) -> String {
    format_source("<memory>", source).unwrap_or_else(|e| panic!("format failed: {e}\n--- source ---\n{source}"))
}

/// Every placement of `//`, `////` and `/* */` comments the formatter must keep.
const COMMENT_CORPUS: &str = r#"// File header line one.
// File header line two: instructions for the exercise.
//
// TODO: make the test pass.

// Comment before the uses.
use Core.Io; // trailing comment on a use
use Core.Text;

/* Block comment before a type. */
pub type Point
{
    // Comment before the first field.
    i32 x; // trailing comment on a field
    /* inline block comment before a field */ i32 y;
    // Comment after the last field.
}

pub enum Shape
{
    Circle, // trailing comment on a variant
    // Comment between variants.
    Square(i32 side),
}

//// Four-slash comment before a contract.
pub contract Measure
{
    // Comment in a contract.
    i32 Area();
}

/// Documentation stays documentation.
pub i32 Main()
{
    // Comment at the start of a body.
    let a = 1; // trailing comment on a statement

    // Comment after a blank line.
    let b = Add(
        a, // comment inside a call
        2);
    if (a < b)
    {
        // Comment inside an if.
        return a;
    }
    else
    {
        return b; // trailing comment in else
    }
    let c = match (a)
    {
        // Comment before an arm.
        1 => 10, // trailing comment on an arm
        _ => 20,
    };
    // Comment at the end of a body.
}

i32 Empty()
{
    // Only a comment in an otherwise empty body.
}

i32 Add(i32 left, i32 right)
{
    return left + right;
}

test adds_numbers
{
    // Comment in a test.
    Testing.Assert.Equal(3, Add(1, 2));
}

// Comment at the end of the file.
"#;

fn comment_lines(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        if let Some(index) = line.find("//") {
            out.push(line[index..].trim_end().to_string());
        }
        if let Some(start) = line.find("/*") {
            let end = line.find("*/").map_or(line.len(), |e| e + 2);
            out.push(line[start..end].to_string());
        }
    }
    out
}

#[test]
fn every_comment_survives_formatting() {
    let formatted = fmt(COMMENT_CORPUS);
    for comment in comment_lines(COMMENT_CORPUS) {
        assert!(formatted.contains(&comment), "comment `{comment}` was lost:\n{formatted}");
    }
}

#[test]
fn formatting_with_comments_is_idempotent() {
    let once = fmt(COMMENT_CORPUS);
    let twice = fmt(&once);
    assert_eq!(once, twice, "second format changed the text");
}

#[test]
fn file_header_comment_survives_formatting() {
    let source = "// Exercise: intro1\n//\n// Make `Answer` return 42.\n// TODO: fix the return value.\n\ni32 Answer() { return 0; }\n";
    let expected = concat!(
        "// Exercise: intro1\n",
        "//\n",
        "// Make `Answer` return 42.\n",
        "// TODO: fix the return value.\n",
        "\n",
        "i32 Answer()\n",
        "{\n",
        "    return 0;\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn comment_only_and_empty_bodies_keep_their_comments() {
    let source = "i32 F() {\n    // TODO\n}\ntype T {\n    // fields later\n}\n";
    let expected =
        concat!("i32 F()\n", "{\n", "    // TODO\n", "}\n", "\n", "type T\n", "{\n", "    // fields later\n", "}\n",);
    assert_eq!(fmt(source), expected);
}

#[test]
fn trailing_comments_stay_on_their_line() {
    let source = "i32 F() {\n    let a = 1; // one\n    return a; // two\n}\n";
    let formatted = fmt(source);
    assert!(formatted.contains("    let a = 1; // one\n"), "{formatted}");
    assert!(formatted.contains("    return a; // two\n"), "{formatted}");
}

#[test]
fn comments_in_strings_are_not_comments() {
    let source = "string F() { return \"http://example.org /* not a comment */\"; }\n";
    let formatted = fmt(source);
    assert_eq!(formatted.matches("http://example.org /* not a comment */").count(), 1, "{formatted}");
}

#[test]
fn parse_error_is_an_error_and_never_repaired() {
    let source = "i32 Main()\n{\n    let x: i32 = 5\n    return x;\n}\n";
    let error = format_source("broken.bd", source).expect_err("a source that does not parse must not format");
    assert!(matches!(error, FormatError::Parse(_)), "{error}");
}

#[test]
fn consecutive_uses_fields_and_variants_stay_together() {
    let source = "use Core.Io;\nuse Core.Text;\ntype P { i32 x; i32 y; }\nenum E { A, B, }\n";
    let expected = concat!(
        "use Core.Io;\n",
        "use Core.Text;\n",
        "\n",
        "type P\n",
        "{\n",
        "    i32 x;\n",
        "    i32 y;\n",
        "}\n",
        "\n",
        "enum E\n",
        "{\n",
        "    A,\n",
        "    B,\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn author_blank_lines_are_kept_once_and_runs_collapse() {
    let source = "use Core.Io;\n\n\n\nuse Core.Text;\ntype P {\n\n    i32 x;\n\n\n    i32 y;\n\n}\n";
    let expected = concat!(
        "use Core.Io;\n",
        "\n",
        "use Core.Text;\n",
        "\n",
        "type P\n",
        "{\n",
        "    i32 x;\n",
        "\n",
        "    i32 y;\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn different_item_kinds_are_separated_like_csharp() {
    let source = "use Core.Io;\ntype A { i32 x; }\ntype B { i32 y; }\ni32 F() { return 1; }\ni32 G() { return 2; }\n";
    let expected = concat!(
        "use Core.Io;\n",
        "\n",
        "type A\n",
        "{\n",
        "    i32 x;\n",
        "}\n",
        "\n",
        "type B\n",
        "{\n",
        "    i32 y;\n",
        "}\n",
        "\n",
        "i32 F()\n",
        "{\n",
        "    return 1;\n",
        "}\n",
        "\n",
        "i32 G()\n",
        "{\n",
        "    return 2;\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn fields_and_methods_are_separated_and_methods_stand_apart() {
    let source = "type C { i32 count; i32 Get() { return 1; } i32 Set() { return 2; } }\n";
    let expected = concat!(
        "type C\n",
        "{\n",
        "    i32 count;\n",
        "\n",
        "    i32 Get()\n",
        "    {\n",
        "        return 1;\n",
        "    }\n",
        "\n",
        "    i32 Set()\n",
        "    {\n",
        "        return 2;\n",
        "    }\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn statements_keep_the_author_grouping_in_functions_and_tests() {
    let source = "test checks_values {\n    Testing.Assert.Equal(1, 1);\n    Testing.Assert.Equal(2, 2);\n    Testing.Assert.Equal(3, 3);\n\n\n    Testing.Assert.Equal(4, 4);\n}\n";
    // The formatter never renames: the naming-style lint (W1637) reports the test name instead.
    let expected = concat!(
        "test checks_values\n",
        "{\n",
        "    Testing.Assert.Equal(1, 1);\n",
        "    Testing.Assert.Equal(2, 2);\n",
        "    Testing.Assert.Equal(3, 3);\n",
        "\n",
        "    Testing.Assert.Equal(4, 4);\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn allman_braces_and_parenthesized_conditions_everywhere() {
    let source = "unit F(i32 v) { if v < 0 { return; } else { return; } while v > 0 { break; } for i in range(0, 3) { continue; } }\n";
    let expected = concat!(
        "unit F(i32 v)\n",
        "{\n",
        "    if (v < 0)\n",
        "    {\n",
        "        return;\n",
        "    }\n",
        "    else\n",
        "    {\n",
        "        return;\n",
        "    }\n",
        "    while (v > 0)\n",
        "    {\n",
        "        break;\n",
        "    }\n",
        "    for i in range(0, 3)\n",
        "    {\n",
        "        continue;\n",
        "    }\n",
        "}\n",
    );
    assert_eq!(fmt(source), expected);
}

#[test]
fn already_formatted_text_is_unchanged() {
    let once = fmt(COMMENT_CORPUS);
    assert_eq!(format_source("<memory>", &once).unwrap(), once, "`--check` and the editor must see 0 changes");
}

#[test]
fn interpolated_strings_are_printed_as_written() {
    let cases = [
        r#""Hello, ${name}!""#,
        r#""${String.Len(s)}""#,
        r#""Hello, ${name}! You have ${n} new.""#,
        r#""<${s}>""#,
        r#""quote \" and ${F("inner \"quoted\" text")} and \\ and \${not a hole}""#,
        r#""tab\tnew line\n and ${a + b} end""#,
    ];
    for literal in cases {
        let source = format!("string F(string name, string s, i32 n, i32 a, i32 b) {{ return {literal}; }}\n");
        let formatted = fmt(&source);
        assert!(formatted.contains(&format!("    return {literal};\n")), "literal {literal} changed:\n{formatted}");
        assert!(!formatted.contains("\" + "), "interpolation lowered to concatenation:\n{formatted}");
        assert_eq!(fmt(&formatted), formatted);
    }
}

#[test]
fn literals_and_operators_keep_their_source_form() {
    let source = "unit F() { mut i32 x = 1; x += 2; x -= 1; x *= 3; x /= 2; x %= 5; char c = '\\n'; char q = '\\''; string s = \"a\\tb\"; return; }\n";
    let formatted = fmt(source);
    for fragment in ["x += 2;", "x -= 1;", "x *= 3;", "x /= 2;", "x %= 5;", "'\\n'", "'\\''", "\"a\\tb\""] {
        assert!(formatted.contains(fragment), "`{fragment}` changed:\n{formatted}");
    }
}

#[test]
fn expression_bodied_methods_stay_expression_bodied() {
    let source = "type C { i32 count; pub i32 Get() => this.count; pub i32 Twice() => this.count * 2; }\n";
    let formatted = fmt(source);
    assert!(formatted.contains("    pub i32 Get() => this.count;\n"), "{formatted}");
    assert!(formatted.contains("    pub i32 Twice() => this.count * 2;\n"), "{formatted}");
    assert!(!formatted.contains("return"), "expression body lowered to a block:\n{formatted}");
}

#[test]
fn token_check_rejects_code_changes() {
    use crate::format::tokens::{code_tokens, first_difference};
    let tokens = |source: &str| code_tokens(source, &[], &[], &[]);
    let before = tokens("a + b");
    let after = tokens("a - b");
    assert!(first_difference(&before, &after).is_some());
    let written = tokens("if (bad_name) { F(x,); }");
    assert_eq!(first_difference(&written, &tokens("if (bad_name) { F(x); }")), None);
    let renamed = tokens("if (BadName) { F(x); }");
    assert!(first_difference(&written, &renamed).is_some(), "a renamed identifier is a code change");
    let respelled = tokens("if (badname) { F(x); }");
    assert!(first_difference(&written, &respelled).is_some(), "a dropped underscore is a code change");
    let more_arguments = tokens("if (bad_name) { F(x, y); }");
    assert!(first_difference(&written, &more_arguments).is_some(), "an added argument is a code change");
    let lowered = tokens("\"a \" + b");
    let written = tokens("\"a ${b}\"");
    assert!(first_difference(&written, &lowered).is_some(), "lowered interpolation is a code change");
}

#[test]
fn token_check_keeps_call_parentheses() {
    use crate::format::tokens::{code_tokens, first_difference};
    let tokens = |source: &str| code_tokens(source, &[], &[], &[]);
    // Only parentheses the parse recorded as omitted may disappear; a call never loses its `()`.
    assert!(first_difference(&tokens("x.Read();"), &tokens("x.Read;")).is_some());
    assert!(first_difference(&tokens("let e = E::None();"), &tokens("let e = E::None;")).is_some());
    let source = "let e = E::None();";
    let open = source.find('(').unwrap();
    assert_eq!(first_difference(&code_tokens(source, &[], &[], &[open]), &tokens("let e = E::None;")), None);
}

#[test]
fn token_check_strips_only_whole_condition_parentheses() {
    use crate::format::tokens::{code_tokens, first_difference};
    let tokens = |source: &str| code_tokens(source, &[], &[], &[]);
    assert_eq!(first_difference(&tokens("if ((a)) { }"), &tokens("if a { }")), None);
    assert_eq!(first_difference(&tokens("while (a && b) { }"), &tokens("while a && b { }")), None);
    // A pair that groups only part of the condition is code: regrouping it changes the program.
    let partial = tokens("if (a || b) && c { }");
    assert!(first_difference(&partial, &tokens("if (a || b && c) { }")).is_some());
    assert!(first_difference(&partial, &tokens("if a || b && c { }")).is_some());
}

#[test]
fn token_check_reads_operators_longest_first() {
    use crate::format::tokens::{code_tokens, first_difference};
    let tokens = |source: &str| code_tokens(source, &[], &[], &[]);
    assert_eq!(tokens("a >> b"), ["a", ">>", "b"]);
    assert_eq!(tokens("(x) => x"), ["x", "=>", "x"]);
    assert_eq!(tokens("E::A"), ["E", "::", "A"]);
    for (written, changed) in [("a >> b", "a > > b"), ("x => y", "x = > y"), ("a === b", "a == = b"), ("a::b", "a: :b")]
    {
        assert!(first_difference(&tokens(written), &tokens(changed)).is_some(), "`{written}` vs `{changed}`");
    }
}

#[test]
fn omitted_parentheses_are_recorded_only_where_the_printer_drops_them() {
    let source = "[Mark()] enum E { A, B(i32 x) } i32 F(E e) { let a = E::A(); let n = Make(); let r = match e { E::A() => 1, E::B(x) => x, }; return r; }\n";
    let formatted = fmt(source);
    assert!(formatted.contains("[Mark]\n"), "{formatted}");
    assert!(formatted.contains("let a = E::A;"), "{formatted}");
    assert!(formatted.contains("let n = Make();"), "{formatted}");
    assert!(formatted.contains("E::A => "), "{formatted}");
    assert!(formatted.contains("E::B(x) => "), "{formatted}");
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn trailing_commas_may_be_dropped_or_added_by_the_printer() {
    let source = "pub attribute Mark(Field, Parameter,) { }\nenum E { A(i32 a, i32 b,), B }\nunit F() { i32[] xs = [1, 2, 3,]; Point p = Point { x: 1, y: 2, }; Point q = Point { x: 3, y: 4 }; return; }\n";
    let formatted = fmt(source);
    assert!(formatted.contains("Mark(Field, Parameter)"), "{formatted}");
    assert!(formatted.contains("A(i32 a, i32 b),\n"), "{formatted}");
    assert!(formatted.contains("    B,\n"), "{formatted}");
    assert!(formatted.contains("[1, 2, 3]"), "{formatted}");
    assert!(formatted.contains("        y: 2,\n"), "{formatted}");
    assert!(formatted.contains("        y: 4,\n"), "{formatted}");
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn code_and_clif_literals_are_printed_as_written() {
    let source = concat!(
        "string F() { let s = code ```beskid\n",
        "pub i32 G() {  return @{ 1 + 2 }; }\n",
        "```\n",
        "; return s; }\n",
        "f64 Floor(f64 x) { return clif {  call @floor(%0) }; }\n",
    );
    let formatted = fmt(source);
    let fence = "    let s = code ```beskid\npub i32 G() {  return @{ 1 + 2 }; }\n```;\n";
    assert!(formatted.contains(fence), "the `;` after a code fence moved:\n{formatted}");
    assert!(formatted.contains("    return clif {  call @floor(%0) };\n"), "{formatted}");
    assert_eq!(fmt(&formatted), formatted);

    // A program without source text prints its code literals with the same fence layout.
    let program = crate::services::parse_program(source).expect("parse");
    let printed = super::format_generated_program(&program).expect("print");
    assert!(printed.contains("code ```beskid\npub i32 G() {  return @{1 + 2}; }\n```;"), "{printed}");
}

#[test]
fn crlf_files_keep_crlf_line_endings() {
    let source = "// Header.\r\n/* Block\r\n   comment. */\r\ni32 F() { return 1; }\r\n";
    let formatted = fmt(source);
    assert_eq!(formatted, "// Header.\r\n/* Block\r\n   comment. */\r\ni32 F()\r\n{\r\n    return 1;\r\n}\r\n");
    assert_eq!(fmt(&formatted), formatted, "`--check` passes on a formatted CRLF file");
    assert!(!formatted.replace("\r\n", "").contains('\n'), "no bare `\\n` line ending:\n{formatted:?}");
}

#[test]
fn a_byte_order_mark_is_kept() {
    let source = "\u{FEFF}i32 F() { return 1; }\n";
    let formatted = fmt(source);
    assert_eq!(formatted, "\u{FEFF}i32 F()\n{\n    return 1;\n}\n");
    assert_eq!(fmt(&formatted), formatted);
    let crlf = fmt("\u{FEFF}i32 F() { return 1; }\r\n");
    assert_eq!(crlf, "\u{FEFF}i32 F()\r\n{\r\n    return 1;\r\n}\r\n");
}

#[test]
fn a_missing_final_newline_is_added() {
    assert_eq!(fmt("i32 F() { return 1; }"), "i32 F()\n{\n    return 1;\n}\n");
    assert_eq!(fmt("i32 F() { return 1; } // last"), "i32 F()\n{\n    return 1;\n} // last\n");
}

#[test]
fn documentation_keeps_paragraphs_indentation_and_blank_lines() {
    let source = "/// Summary.\n///\n/// Example:\n///\n///     let x = F();\n///\nunit F() { return; }\n";
    let formatted = fmt(source);
    assert!(formatted.starts_with(source.strip_suffix("unit F() { return; }\n").unwrap()), "{formatted}");
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn documentation_check_compares_text_after_the_marker() {
    use super::source::doc_line_text;
    assert_eq!(doc_line_text("    /// Text."), "Text.");
    assert_eq!(doc_line_text("///Text."), "Text.", "the printer adds one space after `///`");
    assert_ne!(doc_line_text("///     code"), doc_line_text("/// code"), "indentation inside a doc line counts");
    assert_ne!(doc_line_text("/// a b"), doc_line_text("/// ab"));
    assert_eq!(doc_line_text("///"), "");
}

#[test]
fn comments_between_a_header_and_its_brace_stay_with_the_header() {
    let source = "i32 F() // returns one\n{ return 1; }\ni32 G()\n// note\n/* more */\n{ return 2; }\nunit H(bool x) { if (x) /* why */ { return; } }\n";
    let formatted = fmt(source);
    assert!(formatted.contains("i32 F() // returns one\n{\n    return 1;\n}\n"), "{formatted}");
    assert!(formatted.contains("i32 G()\n// note\n/* more */\n{\n    return 2;\n}\n"), "{formatted}");
    assert!(formatted.contains("    if (x) /* why */\n    {\n        return;\n    }\n"), "{formatted}");
    assert_eq!(fmt(&formatted), formatted);
}

#[test]
fn doubled_commas_between_match_arms_are_formatted() {
    let formatted = fmt("i32 F(i32 x) { return match x { 1 => 10,, _ => 20,, }; }\n");
    assert!(formatted.contains("        1 => 10,\n        _ => 20,\n"), "{formatted}");
    assert_eq!(fmt(&formatted), formatted);
}
