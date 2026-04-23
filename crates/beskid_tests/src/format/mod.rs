//! Formatter (`Emit`) integration tests.

use beskid_analysis::format::format_program;
use beskid_analysis::services::parse_program;

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
