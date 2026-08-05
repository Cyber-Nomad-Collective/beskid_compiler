//! Link-plan lowering must pass `validate_artifact` for project entrypoints.

use std::fs;

use beskid_analysis::CompilationContext;
use beskid_analysis::projects::{AssemblyDiscovery, AssemblyOptions};
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan};
use beskid_codegen::linking::{FunctionDefIndex, LinkPlan};
use beskid_codegen::lowering::lower_program_with_assembly_for_entrypoint;
use beskid_codegen::validate_artifact;
use beskid_queries::{compile_front_end_from_resolved_input, configure_db_for_project, program_assembly, with_db};

use crate::projects::with_cwd;
use crate::test_harness::{temp_case_dir, write_project_manifest as write_manifest};

#[cfg(feature = "slow")]
use crate::projects::fixture_harness::{
    corelib_tests_project_root, lower_corelib_tests_entrypoint, resolve_corelib_tests_entry_with_assembly,
    with_project_test_env,
};
#[cfg(feature = "slow")]
use beskid_codegen::CodegenArtifact;
#[cfg(feature = "slow")]
use beskid_codegen::lowering::lower_program_with_assembly;

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn main_entry_link_plan_validates_for_temp_project() {
    let root = temp_case_dir("spine_link_completeness");
    let src_dir = root.join("Src");
    fs::create_dir_all(&src_dir).expect("source root");
    write_manifest(
        &root,
        r#"
project {
  name = "LinkSmoke"
  version = "0.1.0"
}

target "app" {
  kind = App
  entry = "Main.bd"
}
"#,
    );

    let source = r#"
i32 helper() {
    return 7;
}

i32 Main() {
    return helper();
}
"#;
    let entry = src_dir.join("Main.bd");
    fs::write(&entry, source).expect("write source");

    with_cwd(&root, || {
        let ctx = CompilationContext::try_for_analysis_path(&entry, None).expect("context");
        let plan = ctx.compile_plan.clone().expect("plan");
        let resolved = resolved_input_from_plan(entry.clone(), source.to_string(), plan.clone(), None, None);
        configure_db_for_project(&root);
        let assembly = with_db(|db| {
            program_assembly(
                db,
                &plan,
                resolved.prepared_workspace.as_ref(),
                &entry,
                Some(source),
                &AssemblyOptions { discovery: AssemblyDiscovery::ImportClosure, ..Default::default() },
            )
        })
        .expect("assemble");

        let front =
            compile_front_end_from_resolved_input(&resolved, FrontEndOptions::default(), None).expect("front end");

        let def_index = FunctionDefIndex::build(&front.resolution, &assembly.hir_units);
        let link_plan = LinkPlan::build_for_entrypoint(
            &front.hir,
            "Main",
            Some(&resolved.source_path),
            &front.resolution,
            &front.typed,
            &def_index,
        );
        assert!(!link_plan.entries.is_empty(), "temp project should expose a main entry");

        let artifact = lower_program_with_assembly_for_entrypoint(
            &front.hir,
            &front.resolution,
            &front.typed,
            Some(&assembly),
            Some("Main"),
        )
        .expect("lower");

        validate_artifact(&artifact).expect("link plan symbols must be present in artifact");
    });

    let _ = fs::remove_dir_all(root);
}

/// Corelib `Testing.Assert.Equal` must survive dependency link plans and validate.
#[cfg(feature = "slow")]
#[test]
fn corelib_assert_equal_i64_link_plan_validates() {
    with_project_test_env(&corelib_tests_project_root(), || {
        let resolved = resolve_corelib_tests_entry_with_assembly("collections/ArrayTests.bd");
        let front = compile_front_end_from_resolved_input(
            &resolved,
            FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
            None,
        )
        .expect("front-end");

        let assembly = resolved.assembly.as_ref().expect("assembly");
        let def_index = FunctionDefIndex::build(&front.resolution, &assembly.hir_units);
        let link_plan = LinkPlan::build(&front.hir, &front.resolution, &front.typed, &def_index);
        assert!(
            link_plan.emitted_symbol_names(&front.resolution).iter().any(|name| name.contains("Equal")),
            "link plan should reach Testing.Assert.Equal"
        );

        let artifact = lower_program_with_assembly(&front.hir, &front.resolution, &front.typed, Some(assembly))
            .expect("lower corelib array tests");

        let names: Vec<&str> = artifact.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(
            names.iter().any(|name| name.contains("Equal")),
            "expected Equal in artifact, have {} symbols",
            names.len()
        );
        validate_artifact(&artifact).expect("Assert.Equal link plan must validate");
    });
}

/// The ANSI cursor builder must travel through the production syntax-only link surface.
///
/// This deliberately replaces the retired HIR `LinkPlan` probe. The generated ISLE artifact is
/// the executable link-plan authority: a successful HIR link plan says nothing about whether a
/// `CodegenInput` can select every syntax lowering rule required by the canonical corelib.
#[cfg(feature = "slow")]
#[test]
fn ansi_cursor_builder_home_syntax_isle_link_plan_validates() {
    let artifact = lower_corelib_tests_entrypoint("console/AnsiBuildersTests.bd", "ansi_cursor_builder_home");
    validate_artifact(&artifact).expect("ansi_cursor_builder_home syntax ISLE link plan must validate");

    for expected_symbol in ["Home#syntax_", "IntoSequence#syntax_", "WhenEnabled#syntax_", "Esc#syntax_"] {
        assert!(
            artifact.functions.iter().any(|function| function.name.contains(expected_symbol)),
            "syntax link closure must retain {expected_symbol}; emitted {:?}",
            artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
        );
    }
}

/// Byte-level regression for the normative `ESC [` CSI framing in `Ansi.Escape`.
///
/// The sequence is assembled at runtime, so it has no one `ESC[1;31m` static global. The golden
/// locks the exact ISLE-owned fragments instead: `Esc()` owns the one-byte ESC control character,
/// while the entry test owns the CSI body, final byte, and expected-message suffix.
#[cfg(feature = "slow")]
#[test]
fn ansi_csi_bold_red_syntax_isle_preserves_csi_byte_golden() {
    let artifact = lower_corelib_tests_entrypoint("console/AnsiEscapeTests.bd", "ansi_csi_bold_red");
    validate_artifact(&artifact).expect("ansi_csi_bold_red syntax ISLE link plan must validate");

    assert_literal_byte_goldens(&artifact, &[b"\x1b", b"1;31", b"m", b"[1;31m"]);
}

#[cfg(feature = "slow")]
fn assert_literal_byte_goldens(artifact: &CodegenArtifact, expected_literals: &[&[u8]]) {
    let emitted = artifact.string_literals.values().map(Vec::as_slice).collect::<Vec<_>>();
    for expected in expected_literals {
        assert!(emitted.contains(expected), "missing byte golden {expected:?}; emitted {emitted:?}");
    }
}
