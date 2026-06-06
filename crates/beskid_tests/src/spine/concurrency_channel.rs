//! Concurrency `Channel<T>` and error types must type-check in the front-end.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, compile_front_end_from_resolved_input, resolve_input,
};

use crate::projects::{compiler_workspace_root, with_cwd_at_workspace_root};

#[test]
fn channel_api_tests_front_end_typechecks() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/ChannelApiTests.bd");
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
    let source = fs::read_to_string(&entry).expect("read ChannelApiTests.bd");

    let resolved = with_cwd_at_workspace_root(&root, || {
        resolve_input(Some(&entry), Some(&project_root), None, None, false, false).expect("resolve")
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
    .expect("ChannelApiTests front-end must type-check generic Channel and error types");
}

#[test]
fn wait_group_tests_front_end_typechecks() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/WaitGroupTests.bd");
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
    let source = fs::read_to_string(&entry).expect("read WaitGroupTests.bd");

    let resolved = with_cwd_at_workspace_root(&root, || {
        resolve_input(Some(&entry), Some(&project_root), None, None, false, false).expect("resolve")
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
    .expect("WaitGroupTests front-end must type-check self-parameter methods");
}
