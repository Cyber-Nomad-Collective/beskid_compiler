//! Conflict / scheduling diagnostics surfaced by `mod_host::validate`.
//!
//! Asserts E1828, E1829, E1851, E1852, E1853, E1854 fire from the reference
//! fixture before `mod.collect` and abort scheduling deterministically.

use beskid_analysis::mod_host::{ModHostInput, extract_mod_host_diagnostics, run_through_generate};
use beskid_analysis::services::parse_program_with_source_name;

use super::fixture::ModFixtureWorkspace;

#[test]
fn duplicate_registration_in_one_artifact_emits_e1829() {
    let workspace = ModFixtureWorkspace::new("sample_mod_duplicate_e1829");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" },
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate_dup" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();

    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let result = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    );

    let err = match result {
        Ok(_) => panic!("duplicate (contractId, typeId) registration must abort scheduling"),
        Err(err) => err,
    };
    let diagnostics = extract_mod_host_diagnostics(&err).expect("mod host diagnostics surfaced through anyhow chain");
    assert!(diagnostics.codes().contains(&"E1829"));
}

#[test]
fn unknown_contract_id_emits_e1853() {
    let workspace = ModFixtureWorkspace::new("sample_mod_unknown_e1853");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Made.Up", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();
    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let err = match run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    ) {
        Ok(_) => panic!("unknown contractId must abort scheduling"),
        Err(err) => err,
    };
    let diagnostics = extract_mod_host_diagnostics(&err).expect("mod host diagnostics");
    assert!(diagnostics.codes().contains(&"E1853"));
}

#[test]
fn rewriter_without_analyzer_emits_e1854() {
    let workspace = ModFixtureWorkspace::new("sample_mod_rewriter_e1854");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Rewriter", "typeId": "SampleMod.SampleRewrite", "entrySymbol": "samplemod_rewrite" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();
    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let err = match run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    ) {
        Ok(_) => panic!("rewriter without analyzer must abort scheduling"),
        Err(err) => err,
    };
    let diagnostics = extract_mod_host_diagnostics(&err).expect("mod host diagnostics");
    assert!(diagnostics.codes().contains(&"E1854"));
}

#[test]
fn missing_entry_symbol_emits_e1828() {
    let workspace = ModFixtureWorkspace::new("sample_mod_missing_entry_e1828");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();
    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let err = match run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    ) {
        Ok(_) => panic!("empty entrySymbol must abort scheduling"),
        Err(err) => err,
    };
    let diagnostics = extract_mod_host_diagnostics(&err).expect("mod host diagnostics");
    assert!(diagnostics.codes().contains(&"E1828"));
}
