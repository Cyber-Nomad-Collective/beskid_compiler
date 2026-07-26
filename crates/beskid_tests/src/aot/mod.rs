//! End-to-end AOT tests: codegen artifact → object / link, entrypoints, runtime strategies.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::TargetMetadata;
pub(super) use beskid_abi::{SYM_ABI_VERSION, SYM_INTEROP_DISPATCH_UNIT};
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source};
pub(super) use beskid_aot::{
    AotBuildRequest, AotError, BuildOutputKind, ProjectTargetKind, build, default_output_kind, resolve_entrypoint,
};
use beskid_queries::compile_front_end_from_resolved_input;

mod defaults;
mod entrypoint;
mod object_build;
mod runtime_symbols;

/// Isolated temp directory for AOT outputs (distinct prefix from `test_harness::temp_case_dir`).
fn temp_case_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("time ok").as_nanos();
    let dir = std::env::temp_dir().join(format!("beskid_aot_tests_{name}_{}_{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Minimal valid program source for default AOT samples.
fn sample_program() -> &'static str {
    "unit Main() { }"
}

/// Prepare `sample_program` once, then lower it through the production syntax → CodegenInput → ISLE boundary.
fn lower_sample_artifact() -> beskid_codegen::CodegenArtifact {
    let source = sample_program();
    let dir = temp_case_dir("prepared_syntax_sample");
    let path = dir.join("Main.bd");
    std::fs::write(&path, source).expect("write sample source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved = resolved_input_from_plan(path, source.to_owned(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .expect("prepare sample frontend");
    let aot_target = beskid_aot::target::detect_target(None).expect("detect host AOT target");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == aot_target.triple)
        .expect("host AOT target must have an ABI-v5 metadata contract");
    let artifact =
        beskid_aot::lower_prepared_syntax_entrypoint(&front, "Main", target).expect("lower sample through syntax ISLE");
    let _ = std::fs::remove_dir_all(dir);
    artifact
}
