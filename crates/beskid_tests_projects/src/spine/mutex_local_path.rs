//! `mutex.TryLock()` must resolve as local member call, not module path.

use std::fs;

use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::{Node, Statement};

use crate::projects::compiler_workspace_root;

#[test]
fn typed_let_mutex_name_is_not_split_on_mut_keyword() {
    let program = parse_program_with_source_name("test.bd", "test t { Mutex mutex = Mutex.Create(); }")
        .expect("parse typed let with mutex name");
    let Node::TestDefinition(test) = &program.node.items[0].node else {
        panic!("expected test item");
    };
    let Statement::Let(let_stmt) = &test.node.statements[0].node else {
        panic!("expected let statement");
    };
    assert_eq!(let_stmt.node.name.node.name, "mutex");

    let root = compiler_workspace_root();
    let entry = root.join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/MutexTryLockTests.bd");
    if entry.is_file() {
        let source = fs::read_to_string(&entry).expect("read file");
        let program =
            parse_program_with_source_name(&entry.display().to_string(), &source).expect("parse MutexTryLockTests.bd");
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
