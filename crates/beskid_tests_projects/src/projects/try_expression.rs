use beskid_analysis::services::{FrontEndOptions, PrepareOptions};

use crate::projects::fixture_harness::{
    enter_mod_host, mod_invoker_for, resolve_fixture_with_assembly, try_expression_fixture, with_project_test_env,
};

#[test]
fn try_expression_fixture_types_via_semantic_facts() {
    with_project_test_env(&try_expression_fixture(), || {
        let resolved = resolve_fixture_with_assembly(&try_expression_fixture(), "Src/Main.bd", "App");
        let mod_host = enter_mod_host();
        beskid_queries::prepare_compilation(
            &resolved,
            PrepareOptions {
                mod_invoker: Some(mod_invoker_for(&mod_host, &resolved)),
                front_end: FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
                ..Default::default()
            },
            None,
        )
        .expect("try_expression should resolve and type-check through semantic facts");
    });
}
