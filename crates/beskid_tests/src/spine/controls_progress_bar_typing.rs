//! `use Console.Controls.ProgressBar; ProgressBar.ProgressBar.*` must resolve in corelib tests.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::services::{
    compile_front_end_from_resolved_input, resolve_input, FrontEndOptions, ResolvedInput,
};

use crate::projects::with_cwd_at_workspace_root;

fn compiler_workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("compiler workspace root")
        .to_path_buf()
}

#[test]
fn controls_progress_bar_tests_front_end_typechecks() {
    let root = compiler_workspace_root();
    let entry = root.join(
        "corelib/beskid_corelib/tests/corelib_tests/src/console/ControlsProgressBarTests.bd",
    );
    if !entry.is_file() {
        return;
    }
    let project_root = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let source = fs::read_to_string(&entry).expect("read ControlsProgressBarTests.bd");

    let resolved = with_cwd_at_workspace_root(&root, || {
        resolve_input(
            Some(&entry),
            Some(&project_root),
            None,
            None,
            false,
            false,
        )
        .expect("resolve")
    });

    let plan = resolved.compile_plan.expect("compile plan");
    let resolved_input = ResolvedInput {
        source_path: entry,
        source,
        compile_plan: Some(plan),
        prepared_workspace: resolved.prepared_workspace,
        workspace_summary: resolved.workspace_summary,
        assembly: None,
    };

    compile_front_end_from_resolved_input(
        &resolved_input,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("ControlsProgressBarTests front-end must type-check");
}
