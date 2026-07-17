//! Execution proof for a runtime kit published by the production native-host builder.

use std::path::Path;

use beskid_abi::runtime_kit::BuildProfile;
use beskid_analysis::services::{
    FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_engine::services::run_entrypoint_from_front_end_with_engine;
use beskid_engine::{Engine, host_runtime_target};
use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

struct EnvironmentVariableGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvironmentVariableGuard {
    fn set(key: &'static str, value: &Path) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: this integration target is run serially by its focused invocation, and Drop
        // restores the process environment before the test exits.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
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

#[test]
fn fresh_native_runtime_kit_executes_a_canonical_entrypoint() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let target = host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(prefix.path(), target, BuildProfile::Debug)
        .expect("load the exact fresh runtime kit");

    let source = "i64 Main() { return 41 + 1; }";
    let source_path = beskid_codegen::materialize_source_path_for_lowering(
        Path::new("native-runtime-kit-smoke.bd"),
        source,
    )
    .expect("materialize canonical source");
    let resolved = resolved_input_from_plan(
        source_path,
        source.to_owned(),
        synthetic_compile_plan_for_source(Path::new("native-runtime-kit-smoke.bd")),
        None,
        None,
    );
    let front = beskid_queries::compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions::default(),
        None,
    )
    .expect("prepare canonical entrypoint");

    let output = run_entrypoint_from_front_end_with_engine(
        &mut engine,
        &front,
        "native-runtime-kit-smoke.bd",
        source,
        "Main",
        None,
    )
    .expect("execute against the fresh runtime kit");
    assert_eq!(output, "42");
}

#[test]
fn public_run_entrypoint_uses_the_syntax_isle_path() {
    let prefix = tempfile::tempdir().expect("fresh runtime-kit prefix");
    build_native_host(prefix.path().to_path_buf(), RuntimeKitProfile::Debug)
        .expect("publish canonical native runtime kit");
    let _runtime_prefix = EnvironmentVariableGuard::set("BESKID_RUNTIME_PREFIX", prefix.path());

    let output = beskid_engine::services::run_entrypoint(
        Path::new("public-syntax-entrypoint.bd"),
        "i64 Main() { return 41 + 1; }",
        "Main",
    )
    .expect("public entrypoint executes through syntax ISLE");
    assert_eq!(output, "42");
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[ignore = "requires the staged Linux native runtime-kit prefix"]
fn staged_linux_runtime_kit_executes_a_canonical_entrypoint() {
    let prefix = std::env::var_os("BESKID_RUNTIME_PREFIX")
        .map(std::path::PathBuf::from)
        .expect("Linux evidence must set BESKID_RUNTIME_PREFIX");
    let profile = match std::env::var("BESKID_RUNTIME_KIT_PROFILE").as_deref() {
        Ok("debug") => BuildProfile::Debug,
        Ok("release") => BuildProfile::Release,
        value => panic!("unsupported staged runtime profile: {value:?}"),
    };
    let target = host_runtime_target().expect("supported native host target");
    let mut engine = Engine::with_runtime_kit(&prefix, target, profile)
        .expect("load the staged Linux runtime kit");
    let source = "i64 Main() { return 41 + 1; }";
    let source_path = beskid_codegen::materialize_source_path_for_lowering(
        Path::new("staged-linux-runtime-kit-smoke.bd"),
        source,
    )
    .expect("materialize canonical source");
    let resolved = resolved_input_from_plan(
        source_path,
        source.to_owned(),
        synthetic_compile_plan_for_source(Path::new("staged-linux-runtime-kit-smoke.bd")),
        None,
        None,
    );
    let front = beskid_queries::compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions::default(),
        None,
    )
    .expect("prepare canonical entrypoint");
    let output = run_entrypoint_from_front_end_with_engine(
        &mut engine,
        &front,
        "staged-linux-runtime-kit-smoke.bd",
        source,
        "Main",
        None,
    )
    .expect("execute against the staged Linux runtime kit");
    assert_eq!(output, "42");
}
