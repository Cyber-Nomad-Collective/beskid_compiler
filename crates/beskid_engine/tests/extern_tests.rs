#![cfg(target_os = "linux")]

use anyhow::Result;
use beskid_engine::Engine;
use beskid_engine::services::prepare_jit_entrypoint;
use std::sync::Mutex;

static ENGINE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn extern_call_resolves_from_process_symbols() -> Result<()> {
    let _guard = ENGINE_TEST_LOCK.lock().expect("exclusive runtime state");
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 getpid();
}

pub i64 Main() { return C.getpid(); }
"#;
    let prepared =
        prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main", &beskid_engine::ModHost::NoMods)?;
    let mut engine = Engine::new();
    engine.compile_artifact(&prepared.artifact).expect("compile via process-linked libc symbols");
    let main_ptr = unsafe { engine.entrypoint_ptr(&prepared.symbol).unwrap() };
    let fun: extern "C" fn() -> i64 = unsafe { std::mem::transmute(main_ptr) };
    assert!(fun() > 1);
    Ok(())
}

#[test]
fn extern_missing_symbol_errors() -> Result<()> {
    let _guard = ENGINE_TEST_LOCK.lock().expect("exclusive runtime state");
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 no_such_symbol();
}

pub i64 Main() { return C.no_such_symbol(); }
"#;
    let prepared =
        prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main", &beskid_engine::ModHost::NoMods)?;
    let mut engine = Engine::new();
    let err = engine.compile_artifact(&prepared.artifact).expect_err("missing symbol should error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("dlsym("));
    Ok(())
}

/// A symbol the process lacks is looked up in the declared library; a library that cannot be
/// loaded fails the compile.
#[test]
fn extern_missing_library_errors() -> Result<()> {
    let _guard = ENGINE_TEST_LOCK.lock().expect("exclusive runtime state");
    let src = r#"
[Extern(Abi:"C", Library:"libnope.so")]
pub contract C {
    i64 beskid_absent_symbol();
}

pub i64 Main() { return C.beskid_absent_symbol(); }
"#;
    let prepared =
        prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main", &beskid_engine::ModHost::NoMods)?;
    let mut engine = Engine::new();
    let err = engine.compile_artifact(&prepared.artifact).expect_err("missing library should error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("dlopen(libnope.so)"), "{msg}");
    Ok(())
}
