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
    let expected = concat!(
        "i32 F()\n",
        "{\n",
        "    // TODO\n",
        "}\n",
        "\n",
        "type T\n",
        "{\n",
        "    // fields later\n",
        "}\n",
    );
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
    // Test names are written in PascalCase (W1637); the formatter applies the naming style.
    let expected = concat!(
        "test ChecksValues\n",
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
