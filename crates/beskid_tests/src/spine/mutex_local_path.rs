//! `mutex.TryLock()` must resolve as local member call, not module path.

use std::fs;
use std::path::PathBuf;

use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, compile_front_end_from_resolved_input,
    parse_program_with_source_name, resolve_input,
};
use beskid_analysis::syntax::{Node, Statement};

use crate::projects::{compiler_workspace_root, with_cwd_at_workspace_root};

#[test]
fn typed_let_mutex_name_is_not_split_on_mut_keyword() {
    let program =
        parse_program_with_source_name("test.bd", "test t { Mutex mutex = Mutex.Create(); }")
            .expect("parse typed let with mutex name");
    let Node::TestDefinition(test) = &program.node.items[0].node else {
        panic!("expected test item");
    };
    let Statement::Let(let_stmt) = &test.node.statements[0].node else {
        panic!("expected let statement");
    };
    assert_eq!(let_stmt.node.name.node.name, "mutex");

    let root = compiler_workspace_root();
    let entry = root
        .join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/MutexTryLockTests.bd");
    if entry.is_file() {
        let source = fs::read_to_string(&entry).expect("read file");
        let program = parse_program_with_source_name(&entry.display().to_string(), &source)
            .expect("parse MutexTryLockTests.bd");
        let test = program
            .node
            .items
            .iter()
            .find_map(|item| {
                let Node::TestDefinition(test) = &item.node else {
                    return None;
                };
                Some(test)
            })
            .expect("expected test item");
        let Statement::Let(let_stmt) = &test.node.statements[0].node else {
            panic!("expected let");
        };
        assert_eq!(let_stmt.node.name.node.name, "mutex");
    }
}

#[test]
fn mutex_try_lock_tests_front_end_typechecks() {
    let root = compiler_workspace_root();
    let entry = root
        .join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/MutexTryLockTests.bd");
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
    let source = fs::read_to_string(&entry).expect("read MutexTryLockTests.bd");

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
    .expect("MutexTryLockTests front-end must type-check");
}
