//! Import-closure assembly must surface console shard modules (Ansi.Escape).

use std::fs;

use beskid_analysis::services::resolve_input;
use beskid_queries::{
    configure_db_for_project, program_assembly, with_db,
};

use crate::projects::{compiler_workspace_root, with_cwd_at_workspace_root};

#[test]
fn ansi_escape_resolves_under_corelib_test_assembly() {
    let root = compiler_workspace_root();
    let entry =
        root.join("corelib/beskid_corelib/tests/corelib_tests/src/console/AnsiEscapeTests.bd");
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
        resolve_input(Some(&entry), Some(&project_root), None, None, false, false).expect("resolve")
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
            &beskid_analysis::projects::assembly_options_for_plan(&plan),
        )
    })
    .expect("assemble");

    assert!(
        assembly.units.iter().any(|unit| {
            unit.path.to_string_lossy().contains("Ansi")
                && unit.path.to_string_lossy().contains("Escape")
        }),
        "assembly must include Ansi.Escape via import closure, got {} units",
        assembly.units.len()
    );
}
