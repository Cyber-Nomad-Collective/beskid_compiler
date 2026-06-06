//! Materialized dependency sources must parse the same as package sources.

use std::path::PathBuf;

use beskid_analysis::projects::AssemblyDiscovery;
use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::services::resolve_input;

use crate::projects::{compiler_workspace_root, with_cwd_at_workspace_root};

#[test]
fn concurrency_source_and_materialized_parse_equally() {
    let root = compiler_workspace_root();
    let source_path = root.join("corelib/packages/concurrency/src/Concurrency.bd");
    let source = std::fs::read_to_string(&source_path).expect("read Concurrency.bd");
    parse_program_with_source_name(&source_path.display().to_string(), &source)
        .expect("source Concurrency.bd must parse");

    let entry = root
        .join("corelib/beskid_corelib/tests/corelib_tests/src/concurrency/MutexTryLockTests.bd");
    let project_root = entry
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let entry_source = std::fs::read_to_string(&entry).expect("read test entry");

    let resolved = with_cwd_at_workspace_root(&root, || {
        resolve_input(Some(&entry), Some(&project_root), None, None, false, false)
            .expect("resolve with materialization")
    });

    let materialized = resolved
        .prepared_workspace
        .as_ref()
        .and_then(|ws| {
            ws.materialized_dependencies
                .iter()
                .find(|dep| dep.dependency_name == "corelib_concurrency")
                .map(|dep| dep.materialized_source_root.join("Concurrency.bd"))
        })
        .expect("materialized concurrency unit");
    let materialized_source =
        std::fs::read_to_string(&materialized).expect("read materialized Concurrency.bd");
    assert_eq!(
        source, materialized_source,
        "materialized Concurrency.bd must match package source verbatim"
    );
    parse_program_with_source_name(&materialized.display().to_string(), &materialized_source)
        .expect("materialized Concurrency.bd must parse");
    let _ = AssemblyDiscovery::ImportClosure;
    let _ = entry_source;
}
