use beskid_analysis::projects::{AssemblyDiscovery, AssemblyOptions, assemble_program};
use beskid_analysis::services::{lower_normalize_resolve_type_spanned_with_assembly, resolve_input};

use crate::projects::std_dependency_env_lock;
use crate::projects::test_cwd::{compiler_workspace_root, with_cwd_at_workspace_root};

#[test]
fn try_expression_fixture_lowers_via_program_assembly() {
    with_cwd_at_workspace_root(&compiler_workspace_root(), || {
        let _env_guard = std_dependency_env_lock();
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../beskid_e2e_tests/fixtures/try_expression");
        let resolved = resolve_input(
            Some(&fixture.join("Src/Main.bd")),
            Some(&fixture),
            Some("App"),
            None,
            false,
            false,
        )
        .expect("resolve try_expression fixture");

        let options = AssemblyOptions {
            discovery: AssemblyDiscovery::ImportClosure,
            ..Default::default()
        };
        let assembly = assemble_program(
            &resolved.compile_plan.expect("compile plan"),
            resolved.prepared_workspace.as_ref(),
            &resolved.source_path,
            Some(&resolved.source),
            &options,
        )
        .expect("assemble try_expression");

        let paths = assembly.module_index.known_module_path_strings();
        assert!(
            paths.iter().any(|path| path.contains("Results")),
            "expected Std core Results module in assembly, got: {paths:?}"
        );

        lower_normalize_resolve_type_spanned_with_assembly(
            &assembly.entry_unit().program,
            Some(&assembly),
        )
        .expect("try_expression should resolve and type-check with implicit Std assembly");
    });
}
