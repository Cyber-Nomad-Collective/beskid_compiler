use std::path::Path;

use beskid_engine::services::run_entrypoint;

#[test]
fn jit_repeat_string_accumulation() {
    let source = r#"
string Repeat(string unit, i64 count) {
    string acc = "";
    i64 i = 0;
    while i < count {
        acc = "${acc}${unit}";
        i = i + 1;
    }
    return acc;
}
pub i64 Main() { return __str_len(Repeat("-", 4)); }
"#;
    let output = run_entrypoint(Path::new("repeat.bd"), source, "Main").expect("main should run");
    assert_eq!(output, "4", "expected accumulated string length 4, got {output}");
}
