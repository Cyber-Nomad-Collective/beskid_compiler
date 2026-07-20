#![cfg(target_os = "linux")]

use anyhow::Result;
use beskid_engine::Engine;
use beskid_engine::services::prepare_jit_entrypoint;

#[cfg(feature = "extern_dlopen")]
const LIBC: &str = "libc.so.6";

#[test]
#[cfg(feature = "extern_dlopen")]
fn extern_resolution_only_compiles_with_feature() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 write(i64 fd, ref u8 buf, i64 len);
}

pub i64 Main() { return 0; }
"#;
    let prepared = prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main")?;
    assert!(
        prepared
            .artifact
            .extern_imports
            .iter()
            .any(|e| e.symbol == "write" && e.library.as_deref() == Some(LIBC))
    );

    let mut engine = Engine::new();
    engine
        .compile_artifact(&prepared.artifact)
        .expect("compile with extern_dlopen");
    Ok(())
}

#[test]
#[cfg(not(feature = "extern_dlopen"))]
fn extern_resolution_via_process_symbols_without_feature() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 getpid();
}

pub i64 Main() { return C.getpid(); }
"#;
    let prepared = prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main")?;
    let mut engine = Engine::new();
    engine
        .compile_artifact(&prepared.artifact)
        .expect("compile via process-linked libc symbols");
    Ok(())
}

#[test]
#[cfg(not(feature = "extern_dlopen"))]
fn extern_missing_symbol_errors_without_feature() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 no_such_symbol();
}

pub i64 Main() { return C.no_such_symbol(); }
"#;
    let prepared = prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main")?;
    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&prepared.artifact)
        .expect_err("missing symbol should error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("dlsym("));
    Ok(())
}

#[test]
#[cfg(feature = "extern_dlopen")]
fn extern_real_call_getpid() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 getpid();
}

pub i64 Main() { return C.getpid(); }
"#;
    let prepared = prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main")?;
    let mut engine = Engine::new();
    engine
        .compile_artifact(&prepared.artifact)
        .expect("compile extern call");
    let main_ptr = unsafe { engine.entrypoint_ptr(&prepared.symbol).unwrap() };
    let fun: extern "C" fn() -> i64 = unsafe { std::mem::transmute(main_ptr) };
    let pid = fun();
    assert!(pid > 1);
    Ok(())
}

#[test]
#[cfg(feature = "extern_dlopen")]
fn extern_missing_symbol_errors() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 no_such_symbol();
}

pub i64 Main() { return C.no_such_symbol(); }
"#;
    let prepared = prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main")?;
    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&prepared.artifact)
        .expect_err("missing symbol should error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("dlsym("));
    Ok(())
}

#[test]
#[cfg(feature = "extern_dlopen")]
fn extern_missing_library_errors() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libnope.so")]
pub contract C {
    i64 getpid();
}

pub i64 Main() { return C.getpid(); }
"#;
    let prepared = prepare_jit_entrypoint(std::path::Path::new("<memory>"), src, "Main")?;
    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&prepared.artifact)
        .expect_err("missing library should error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("dlopen("));
    Ok(())
}
