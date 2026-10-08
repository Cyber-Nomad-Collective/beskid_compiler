//! The canonical public Dynamic unit declares private bridge items that only the compiler names
//! (`beskid_queries::semantic_contract::{dynamic_pack, reserved_failure}`). The legacy
//! unused-private-item rule exempts exactly those items, and only in exactly that unit.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::services::semantic_rule_diagnostics_for_program_with_pipeline;

use crate::projects::fixture_harness::{resolve_fixture_with_assembly, with_project_test_env};

const COMPILER_ISSUED_BRIDGE_ITEMS: [&str; 5] =
    ["ReserveAllocationFailureV1", "PackedResultV1", "PackedInvalidV1", "UnpackedResultV1", "UnpackedInvalidV1"];

fn unused_private_items(diagnostics: &[beskid_analysis::SemanticDiagnostic]) -> Vec<&'static str> {
    COMPILER_ISSUED_BRIDGE_ITEMS
        .into_iter()
        .filter(|name| {
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code.as_deref() == Some("W1504") && diagnostic.message.contains(&format!("`{name}`"))
            })
        })
        .collect()
}

#[test]
fn v06_canonical_dynamic_bridge_items_are_not_unused_private_items() {
    let root = super::foundation_root();
    with_project_test_env(&root, || {
        let resolved = resolve_fixture_with_assembly(&root, "src/Core/Dynamic/Dynamic.bd", "FoundationLib");
        let assembly = resolved.assembly.expect("foundation assembly");
        let unit = assembly
            .units
            .iter()
            .find(|unit| assembly.is_canonical_public_dynamic_unit(unit))
            .expect("the foundation package carries the canonical public Dynamic unit");

        let mut canonical = AnalysisOptions::default();
        canonical.entry_source_path = Some(unit.path.clone());
        canonical.program_assembly = Some(assembly.clone());
        let diagnostics = semantic_rule_diagnostics_for_program_with_pipeline(
            &unit.program.node,
            unit.logical_name.clone(),
            &unit.source,
            canonical,
            None,
        );
        assert!(
            unused_private_items(&diagnostics).is_empty(),
            "compiler-issued bridge items must not be reported as unused: {diagnostics:#?}"
        );

        // The same bytes without the canonical unit authority get no exemption: the rule keys on
        // the exact unit, never on the item names alone.
        let unanchored = semantic_rule_diagnostics_for_program_with_pipeline(
            &unit.program.node,
            unit.logical_name.clone(),
            &unit.source,
            AnalysisOptions::default(),
            None,
        );
        assert_eq!(unused_private_items(&unanchored), COMPILER_ISSUED_BRIDGE_ITEMS.to_vec());
    });
}
