use std::path::Path;

use beskid_engine::Engine;
use beskid_engine::services::run_entrypoint;

#[test]
fn jit_repeat_string_accumulation_with_mut() {
    let source = r#"
pub string Repeat(string unit, i64 count) {
    mut string acc = "";
    mut i64 i = 0;
    while i < count {
        acc = "${acc}${unit}";
        i = i + 1;
    }
    return acc;
}
pub i64 Main() { return __str_len(Repeat("-", 4)); }
"#;
    let output = run_entrypoint(Path::new("repeat.bd"), source, "Main").expect("main should run");
    assert_eq!(
        output, "4",
        "expected accumulated string length 4, got {output}"
    );
}

#[test]
fn jit_repeat_string_accumulation_without_mut() {
    let source = r#"
pub string Repeat(string unit, i64 count) {
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
    let lowered =
        beskid_codegen::lower_source(Path::new("repeat.bd"), source, false).expect("lower");
    let main_symbol = lowered
        .artifact
        .functions
        .iter()
        .find(|f| f.name.starts_with("Main#"))
        .map(|f| f.name.as_str())
        .expect("main function");
    let mut engine = Engine::new();
    engine
        .compile_artifact(&lowered.artifact)
        .expect("jit compile");
    let ptr = unsafe { engine.entrypoint_ptr(main_symbol) }.expect("main ptr");
    let main: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    let len = main();
    assert_eq!(len, 4, "expected repeat length 4, got {len}");
}

#[test]
fn jit_repeat_cross_module_string_len_without_mut() {
    let source = r#"
mod Frame {
    pub string Repeat(string unit, i64 count) {
        string acc = "";
        i64 i = 0;
        while i < count {
            acc = "${acc}${unit}";
            i = i + 1;
        }
        return acc;
    }
}
pub i64 Main() { return __str_len(Frame.Repeat("-", 4)); }
"#;
    let output = run_entrypoint(Path::new("repeat.bd"), source, "Main").expect("main should run");
    assert_eq!(
        output, "4",
        "expected cross-module repeat length 4, got {output}"
    );
}
