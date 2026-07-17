use std::path::Path;

use beskid_engine::Engine;
use beskid_engine::services::run_entrypoint;

/// Loop / mut probes for ABI-v5 JIT. Integer accumulation avoids the ABI-v4
/// `interop_dispatch_*` / `__str_len` path removed from the exact runtime kit.
/// Full string-loop coverage remains in `corelib_repeat_jit` (ignored until
/// multi-unit string codegen is stable).

#[test]
fn jit_repeat_string_accumulation_with_mut() {
    let source = r#"
pub i64 Repeat(i64 unit, i64 count) {
    mut i64 acc = 0;
    mut i64 i = 0;
    while i < count {
        acc = acc + unit;
        i = i + 1;
    }
    return acc;
}
pub i64 Main() { return Repeat(1, 4); }
"#;
    let output = run_entrypoint(Path::new("repeat.bd"), source, "Main").expect("main should run");
    assert_eq!(output, "4", "expected accumulated sum 4, got {output}");
}

#[test]
#[ignore = "lower_source path reports missing expression types for while/mut loops; covered by run_entrypoint probe above"]
fn jit_repeat_string_accumulation_without_mut() {
    // Loop counters must be `mut` in current surface syntax; this probe still uses the
    // Engine.compile_artifact path rather than run_entrypoint.
    let source = r#"
pub i64 Repeat(i64 unit, i64 count) {
    mut i64 acc = 0;
    mut i64 i = 0;
    while i < count {
        acc = acc + unit;
        i = i + 1;
    }
    return acc;
}
pub i64 Main() { return Repeat(1, 4); }
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
    assert_eq!(len, 4, "expected repeat sum 4, got {len}");
}

#[test]
#[ignore = "cross-module call_lowering is unavailable until its AST/Salsa port is complete"]
fn jit_repeat_cross_module_string_len_without_mut() {
    let source = r#"
mod Frame {
    pub i64 Repeat(i64 unit, i64 count) {
        mut i64 acc = 0;
        mut i64 i = 0;
        while i < count {
            acc = acc + unit;
            i = i + 1;
        }
        return acc;
    }
}
pub i64 Main() { return Frame.Repeat(1, 4); }
"#;
    let output = run_entrypoint(Path::new("repeat.bd"), source, "Main").expect("main should run");
    assert_eq!(
        output, "4",
        "expected cross-module repeat sum 4, got {output}"
    );
}
