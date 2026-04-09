#![cfg(target_os = "linux")]

use anyhow::Result;
use beskid_codegen::services::lower_source;
use beskid_engine::Engine;

const LIBC: &str = "libc.so.6";

#[test]
#[cfg(feature = "extern_dlopen")]
fn extern_resolution_only_compiles_with_feature() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 write(i64 fd, ref u8 buf, i64 len);
}

pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;
    // Assert extern_imports recorded
    assert!(
        lowered
            .artifact
            .extern_imports
            .iter()
            .any(|e| e.symbol == "write" && e.library.as_deref() == Some(LIBC))
    );

    let mut engine = Engine::new();
    engine
        .compile_artifact(&lowered.artifact)
        .expect("compile with extern_dlopen");
    Ok(())
}

#[test]
#[cfg(not(feature = "extern_dlopen"))]
fn extern_resolution_fails_without_feature() -> Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C {
    i64 write(i64 fd, ref u8 buf, i64 len);
}

pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;
    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
        .expect_err("should fail without feature");
    let msg = format!("{:?}", err);
    assert!(msg.contains("extern_dlopen feature disabled"));
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

pub i64 main() { return C.getpid(); }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;
    let mut engine = Engine::new();
    engine
        .compile_artifact(&lowered.artifact)
        .expect("compile extern call");
    let main_ptr = unsafe { engine.entrypoint_ptr("main").unwrap() };
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

pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;
    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
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

pub i64 main() { return 0; }
"#;
    let lowered = lower_source(std::path::Path::new("<memory>"), src, false)?;
    let mut engine = Engine::new();
    let err = engine
        .compile_artifact(&lowered.artifact)
        .expect_err("missing library should error");
    let msg = format!("{:?}", err);
    assert!(msg.contains("dlopen("));
    Ok(())
}
