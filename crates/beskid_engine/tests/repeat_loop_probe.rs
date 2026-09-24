use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_engine::services::{prepare_jit_entrypoint, run_entrypoint};
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

/// Two tests in this binary override `BESKID_RUNTIME_PREFIX`; the default parallel harness let
/// one test's restore remove the variable while the other was still resolving its kit, which
/// surfaced as "current executable is not installed under `<prefix>/bin`". Serialize the override.
static ENVIRONMENT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvironmentVariableGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: &Path) -> Self {
        let lock = ENVIRONMENT_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::var_os(key);
        // SAFETY: every environment override in this binary holds `ENVIRONMENT_LOCK`, and Drop
        // restores the process environment before the lock is released.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous, _lock: lock }
    }
}

impl Drop for EnvironmentVariableGuard {
    fn drop(&mut self) {
        // SAFETY: restores the exact pre-test state established by `EnvironmentVariableGuard::set`.
        unsafe {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}

/// Loop / mut probes for ABI-v5 JIT. Integer accumulation avoids the ABI-v4
/// The multi-unit string-loop regression lives in `corelib_repeat_jit`.

#[test]
fn jit_repeat_string_accumulation_with_mut() {
    let prefix = tempfile::tempdir().expect("exact kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native kit");
    let _runtime_prefix = EnvironmentVariableGuard::set("BESKID_RUNTIME_PREFIX", prefix.path());

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
    let prefix = tempfile::tempdir().expect("exact kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native kit");
    let target = host_runtime_target().expect("host target");
    let mut engine = Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug).expect("load exact kit");

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
    let prefix = tempfile::tempdir().expect("exact kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug).expect("publish exact native kit");
    let _runtime_prefix = EnvironmentVariableGuard::set("BESKID_RUNTIME_PREFIX", prefix.path());

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
