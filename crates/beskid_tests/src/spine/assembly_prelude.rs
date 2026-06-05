//! Assembly prelude seeding must surface console shard modules (Ansi.Escape).

use std::fs;
use std::path::PathBuf;

use beskid_analysis::services::resolve_input;
use beskid_queries::{
    configure_db_for_project, prepare_compilation_with_db, program_assembly, with_db,
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
fn ansi_escape_resolves_under_corelib_test_assembly() {
    let root = compiler_workspace_root();
    let entry = root.join(
        "corelib/beskid_corelib/tests/corelib_tests/src/console/AnsiEscapeTests.bd",
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
    let source = fs::read_to_string(&entry).expect("read AnsiEscapeTests.bd");

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
    configure_db_for_project(&project_root);
    let assembly = with_db(|db| {
        program_assembly(
            db,
            &plan,
            resolved.prepared_workspace.as_ref(),
            &entry,
            Some(&source),
            &Default::default(),
        )
    })
    .expect("assemble");

    assert!(
        assembly.units.iter().any(|unit| {
            unit.path
                .to_string_lossy()
                .contains("Ansi")
                && unit.path.to_string_lossy().contains("Escape")
        }),
        "assembly must include Ansi.Escape from prelude seeding, got {} units",
        assembly.units.len()
    );
}
