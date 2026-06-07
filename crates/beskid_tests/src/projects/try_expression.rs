use beskid_analysis::services::lower_normalize_resolve_type_spanned_with_assembly;

use crate::projects::fixture_harness::{
    resolve_fixture_with_assembly, try_expression_fixture, with_project_test_env,
};

#[test]
fn try_expression_fixture_lowers_via_program_assembly() {
    with_project_test_env(&try_expression_fixture(), || {
        let resolved =
            resolve_fixture_with_assembly(&try_expression_fixture(), "Src/Main.bd", "App");
        let assembly = resolved.assembly.expect("assembly");
        lower_normalize_resolve_type_spanned_with_assembly(
            &assembly.entry_unit().program,
            Some(&assembly),
            None,
        )
        .expect("try_expression should resolve and type-check with implicit Std assembly");
    });
}
