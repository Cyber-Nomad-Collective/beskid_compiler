use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::{prepare_jit_entrypoint, run_entrypoint};
use beskid_engine::{Engine, host_runtime_target};
use crate::runtime_prefix::RuntimePrefixContext;
use crate::shared_kit;

/// Loop / mut probes for ABI-v5 JIT. Integer accumulation avoids the ABI-v4
/// The multi-unit string-loop regression lives in `corelib_repeat_jit`.

#[test]
fn jit_repeat_string_accumulation_with_mut() {
    let kit_lease = shared_kit::debug();
    let prefix = kit_lease.prefix();
    let _runtime_prefix = RuntimePrefixContext::install(prefix);

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
fn jit_repeat_accumulation_via_codegen_input_compile_artifact() {
    let kit_lease = shared_kit::debug();
    let prefix = kit_lease.prefix();
    let target = host_runtime_target().expect("host target");
    let mut engine = Engine::with_runtime_kit(prefix, target, BuildProfile::Debug).expect("load exact kit");

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
    let prepared = prepare_jit_entrypoint(Path::new("repeat.bd"), source, "Main").expect("CodegenInput prepare");
    engine.compile_artifact(&prepared.artifact).expect("jit compile");
    let ptr = unsafe { engine.entrypoint_ptr(&prepared.symbol) }.expect("main ptr");
    let main: extern "C" fn() -> i64 = unsafe { std::mem::transmute(ptr) };
    let len = main();
    assert_eq!(len, 4, "expected repeat sum 4, got {len}");
}

#[test]
fn jit_repeat_cross_module_string_len_without_mut() {
    let kit_lease = shared_kit::debug();
    let prefix = kit_lease.prefix();
    let _runtime_prefix = RuntimePrefixContext::install(prefix);

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
    assert_eq!(output, "4", "expected cross-module repeat sum 4, got {output}");
}
