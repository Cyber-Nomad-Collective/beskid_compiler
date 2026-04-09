use std::path::Path;

use beskid_analysis::services::analyze_program;

#[test]
fn analyze_program_empty_diagnostics_for_trivial_i32_main() {
    let source = "i32 main() {\n    return 0;\n}\n";
    let diags = analyze_program(Path::new("test.bd"), source).expect("analyze");
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {diags:?}"
    );
}

#[test]
fn analyze_program_reports_unknown_value() {
    let source = "i64 main() {\n    i64 x = missing_name;\n    return 0;\n}\n";
    let diags = analyze_program(Path::new("test.bd"), source).expect("analyze");
    assert!(
        !diags.is_empty(),
        "expected at least one diagnostic for unknown value"
    );
    let joined: String = diags.iter().map(|d| d.message.as_str()).collect::<Vec<_>>().join(" ");
    assert!(
        joined.contains("missing_name") || joined.contains("unknown"),
        "unexpected messages: {joined}"
    );
}
