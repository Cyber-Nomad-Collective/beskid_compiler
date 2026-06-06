//! JIT compile and execute helpers shared by runtime integration tests.

use std::panic::{self, AssertUnwindSafe};

use beskid_codegen::lowering::lower_program;
use beskid_engine::Engine;

use super::pipeline::typecheck_hir;

pub fn compile_artifact(source: &str) -> beskid_codegen::CodegenArtifact {
    let (hir, resolution, typed) = typecheck_hir(source);
    lower_program(&hir, &resolution, &typed).expect("expected codegen lowering to succeed")
}

pub fn compile_jit(source: &str) -> Engine {
    let artifact = compile_artifact(source);
    let func_names: Vec<String> = artifact
        .functions
        .iter()
        .map(|func| func.name.clone())
        .collect();

    let mut engine = Engine::new();
    let compile_result = panic::catch_unwind(AssertUnwindSafe(|| {
        engine
            .compile_artifact(&artifact)
            .expect("expected JIT compile to succeed");
    }));

    if let Err(payload) = compile_result {
        eprintln!("JIT compile panicked for source: {source}");
        eprintln!("JIT artifact functions: {func_names:?}");
        panic::resume_unwind(payload);
    }

    engine
}

pub unsafe fn run_main_i64(engine: &mut Engine) -> i64 {
    run_entrypoint0!(engine, "main", i64)
}

pub unsafe fn run_main_i32(engine: &mut Engine) -> i32 {
    run_entrypoint0!(engine, "main", i32)
}

pub fn jit_run_main_i64(source: &str) -> i64 {
    let mut engine = compile_jit(source);
    unsafe { run_main_i64(&mut engine) }
}

pub fn jit_run_main_i32(source: &str) -> i32 {
    let mut engine = compile_jit(source);
    unsafe { run_main_i32(&mut engine) }
}

pub fn jit_compile_only(source: &str) {
    let _engine = compile_jit(source);
}

macro_rules! run_entrypoint0 {
    ($engine:expr, $entrypoint:expr, $ret:ty) => {{
        let ptr = unsafe { $engine.entrypoint_ptr($entrypoint) }
            .expect("expected entrypoint pointer");
        assert!(!ptr.is_null(), "expected non-null entrypoint pointer");
        $engine.with_runtime(|_, _| unsafe {
            let callable: extern "C" fn() -> $ret = std::mem::transmute(ptr);
            callable()
        })
    }};
}

pub(crate) use run_entrypoint0;
