//! Prove System I/O smoke entries cannot use the retired HIR/`Lowerable` codegen drivers.

use std::path::PathBuf;

use beskid_analysis::services::{FrontEndOptions, ResolvedInput, resolve_input};
use beskid_codegen::RETIRED_HIR_LOWERING_PATH;
use beskid_codegen::lowering::lower_program_with_assembly_for_entrypoint;
use beskid_queries::compile_front_end_from_resolved_input;

fn compiler_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("compiler workspace root")
        .to_path_buf()
}

fn assert_hir_driver_rejected(entry_rel: &str, entrypoint: &str) {
    let root = compiler_workspace_root();
    let entry = root
        .join("corelib/beskid_corelib/tests/corelib_tests/src")
        .join(entry_rel);
    let project_root = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = std::fs::read_to_string(&entry).expect("read entry");

    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&root).expect("chdir");
    let resolved = resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
        .expect("resolve");
    std::env::set_current_dir(previous).expect("restore cwd");

    let plan = resolved.compile_plan.expect("compile plan");

    let resolved_input = ResolvedInput {
        source_path: entry,
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };

    let front = compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("front-end");

    let errors = lower_program_with_assembly_for_entrypoint(
        &front.hir,
        &front.resolution,
        &front.typed,
        Some(&front.assembly),
        Some(entrypoint),
    )
    .expect_err("retired HIR driver must reject without fallback");
    let message = errors
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    assert!(
        message.contains(RETIRED_HIR_LOWERING_PATH),
        "entrypoint {entrypoint} in {entry_rel}: {message}"
    );
}

#[test]
fn lower_system_error_writeline_smoke_rejects_hir_driver() {
    assert_hir_driver_rejected("system/ErrorWriteTests.bd", "error_writeline_smoke");
}

#[test]
fn lower_system_input_read_smoke_rejects_hir_driver() {
    assert_hir_driver_rejected("system/InputReadTests.bd", "input_read_smoke");
}
