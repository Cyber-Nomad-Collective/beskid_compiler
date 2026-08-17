use beskid_analysis::format::format_program;
use beskid_analysis::services::parse_program;

#[test]
fn format_normalizes_type_and_field_case() {
    let src = r#"pub type hub_register {
    bool is_tty,
}
"#;
    let program = parse_program(src).expect("parse");
    let out = format_program(&program).expect("format");
    assert!(out.contains("pub type HubRegister"));
    assert!(out.contains("bool isTty"));
}

#[test]
fn format_naming_normalization_is_idempotent() {
    let src = r#"pub type bad_type {
    i32 bad_field,
}

pub unit bad_fn() {
    let BadLocal = 1;
    return;
}
"#;
    let program = parse_program(src).expect("parse");
    let once = format_program(&program).expect("format once");
    let reparsed = parse_program(&once).expect("re-parse");
    let twice = format_program(&reparsed).expect("format twice");
    assert_eq!(once, twice, "naming normalization must be idempotent");
    assert!(once.contains("pub type BadType"));
    assert!(once.contains("i32 badField"));
    assert!(once.contains("pub unit BadFn()"));
    assert!(once.contains("let badLocal = 1"));
}
