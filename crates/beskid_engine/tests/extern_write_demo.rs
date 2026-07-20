#[test]
#[cfg(target_os = "linux")]
#[cfg(feature = "extern_dlopen")]
fn extern_real_call_write() -> anyhow::Result<()> {
    let src = r#"
[Extern(Abi:"C", Library:"libc.so.6")]
pub contract C { i64 write(i32 fd, i64 buf, i64 len); }

pub i64 Main() {
    return C.write(1, __test_bytes_ptr(), __test_bytes_len());
}
"#;
    let prepared = beskid_engine::services::prepare_jit_entrypoint(
        std::path::Path::new("<memory>"),
        src,
        "Main",
    )?;
    let mut engine = beskid_engine::Engine::new();
    engine
        .compile_artifact(&prepared.artifact)
        .expect("compile extern write");
    let main_ptr = unsafe { engine.entrypoint_ptr(&prepared.symbol).unwrap() };
    let fun: extern "C" fn() -> i64 = unsafe { std::mem::transmute(main_ptr) };
    let written = fun();
    assert_eq!(written, "Hello from libc.write\n".len() as i64);
    Ok(())
}
