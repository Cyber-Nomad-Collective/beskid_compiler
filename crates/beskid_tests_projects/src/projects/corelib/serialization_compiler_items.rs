//! The canonical serialization descriptor and compiled-recipe units declare private factories
//! that only the compiled serialization shape authority (`beskid_queries::serialization_shape`)
//! names, and the descriptor unit's `[Extern]` `CompiledShapeBridge` is a compiler-issued
//! runtime bridge, not a user C import, as is the reader unit's `FieldDecoderBridge`. Every
//! exemption keys on the exact canonical unit.

use beskid_analysis::AnalysisOptions;
use beskid_analysis::services::semantic_rule_diagnostics_for_program_with_pipeline;

use crate::projects::fixture_harness::{resolve_fixture_with_assembly, with_project_test_env};

fn unused_private_items(diagnostics: &[beskid_analysis::SemanticDiagnostic], names: &[&'static str]) -> Vec<&'static str> {
    names
        .iter()
        .copied()
        .filter(|name| {
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code.as_deref() == Some("W1504") && diagnostic.message.contains(&format!("`{name}`"))
            })
        })
        .collect()
}

#[test]
fn v06_canonical_serialization_factories_are_compiler_issued() {
    let root = super::serialization_root();
    with_project_test_env(&root, || {
        let resolved =
            resolve_fixture_with_assembly(&root, "src/Core/Serialization/Compiled.bd", "SerializationLib");
        let assembly = resolved.assembly.expect("serialization assembly");
        let reader_assembly =
            resolve_fixture_with_assembly(&root, "src/Core/Serialization/Reader.bd", "SerializationLib")
                .assembly
                .expect("reader assembly");
        let reader = reader_assembly
            .units
            .iter()
            .find(|unit| unit.path.ends_with("Core/Serialization/Reader.bd"))
            .expect("the reader assembly carries Core/Serialization/Reader.bd");
        assert!(reader_assembly.is_canonical_serialization_reader_unit(reader));
        assert!(reader_assembly.is_runtime_bridge_extern_unit(reader), "FieldDecoderBridge is a runtime bridge");
        for (logical, names) in [
            ("Core/Serialization/Descriptors.bd", &["CompiledRoot", "AttachCompiledExtras"][..]),
            ("Core/Serialization/Compiled.bd", &["ConstructCompiledGraph"][..]),
        ] {
            let unit = assembly
                .units
                .iter()
                .find(|unit| unit.path.ends_with(logical))
                .unwrap_or_else(|| panic!("the serialization assembly carries {logical}"));
            if logical.ends_with("Descriptors.bd") {
                assert!(assembly.is_canonical_serialization_descriptors_unit(unit));
                assert!(assembly.is_runtime_bridge_extern_unit(unit), "CompiledShapeBridge is a runtime bridge");
            } else {
                assert!(assembly.is_canonical_serialization_compiled_unit(unit));
                assert!(!assembly.is_runtime_bridge_extern_unit(unit));
            }

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
                unused_private_items(&diagnostics, names).is_empty(),
                "compiler-issued factories of {logical} must not be reported as unused: {diagnostics:#?}"
            );

            // Without the entry path that identifies the canonical unit, no exemption applies.
            let mut unanchored_options = AnalysisOptions::default();
            unanchored_options.program_assembly = Some(assembly.clone());
            let unanchored = semantic_rule_diagnostics_for_program_with_pipeline(
                &unit.program.node,
                unit.logical_name.clone(),
                &unit.source,
                unanchored_options,
                None,
            );
            assert_eq!(unused_private_items(&unanchored, names), names.to_vec());
        }
    });
}
