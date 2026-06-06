//! Analyzer coverage conformance tests (evidence category 4).
//!
//! Per the platform-spec conformance evidence policy, tests must demonstrate
//! analyzers running after generators in the mod host pipeline. These tests use
//! [`ScriptedContractInvoker`] to inject synthetic generator contributions and
//! analyzer diagnostics, then assert the full pipeline dispatches each contract
//! kind in order and surfaces per-contract outcomes.

use beskid_analysis::mod_host::{
    AnalyzerDiagnostic, AnalyzerSeverity, ModHostInput, ScriptedContractInvoker,
    run_analyze_rewrite_with_invoker, run_through_generate,
};
use beskid_analysis::services::parse_program_with_source_name;

use super::fixture::ModFixtureWorkspace;

#[test]
fn generator_contributions_surface_in_outcomes_and_analyzer_dispatches_afterwards() {
    let workspace = ModFixtureWorkspace::new("analyzer_coverage_full_pipe");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" },
    { "contractId": "Beskid.Compiler.Collect.Analyzer",  "typeId": "SampleMod.SampleAnalyze",   "entrySymbol": "samplemod_analyze" },
    { "contractId": "Beskid.Compiler.Collect.Rewriter",  "typeId": "SampleMod.SampleRewrite",   "entrySymbol": "samplemod_rewrite" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();

    let invoker = ScriptedContractInvoker::new()
        .with_generator_contribution(
            "SampleMod.SampleGenerate",
            vec!["pub fn generated_func() { return 42; }".to_owned()],
        )
        .with_analyzer_diagnostic(
            "SampleMod.SampleAnalyze",
            vec![AnalyzerDiagnostic {
                code: "COV0001".to_owned(),
                message: "analyzer processed post-generate program".to_owned(),
                severity: AnalyzerSeverity::Warning,
            }],
        );

    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: Some(&invoker),
        },
    )
    .expect("generate with contributions");

    // Generator contributions are captured in outcomes.
    assert_eq!(generated.generator_outcomes.len(), 1);
    assert_eq!(
        generated.generator_outcomes[0].type_id,
        "SampleMod.SampleGenerate"
    );
    let gen_contributions = &generated.generator_outcomes[0].contributions;
    assert_eq!(gen_contributions.len(), 1);
    assert!(
        gen_contributions[0].contains("generated_func"),
        "generator must contribute `generated_func`; got {:?}",
        gen_contributions
    );

    // Proceed through the semantic gate to analyze/rewrite.
    let snapshot =
        beskid_analysis::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
            .with_composition(&generated.session.composition_snapshot_or_default());
    let analyze = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        Some(&invoker),
        Some(&snapshot),
        None,
    )
    .expect("analyze/rewrite");

    // Analyzer dispatched with correct type_id and surface diagnostics.
    assert_eq!(
        analyze.analyzer_outcomes.len(),
        1,
        "expected exactly one analyzer outcome"
    );
    assert_eq!(
        analyze.analyzer_outcomes[0].type_id, "SampleMod.SampleAnalyze",
        "analyzer type_id must match registration"
    );
    assert_eq!(
        analyze.analyzer_outcomes[0].diagnostics.len(),
        1,
        "scripted analyzer diagnostic must surface"
    );
    assert_eq!(
        analyze.analyzer_outcomes[0].diagnostics[0].code, "COV0001",
        "diagnostic code must match scripted value"
    );

    // Rewriter dispatched after analyzer.
    assert_eq!(
        analyze.rewriter_outcomes.len(),
        1,
        "expected exactly one rewriter outcome"
    );
    assert_eq!(
        analyze.rewriter_outcomes[0].type_id, "SampleMod.SampleRewrite",
        "rewriter type_id must match registration"
    );
}

#[test]
fn multiple_generators_and_analyzers_dispatch_in_order() {
    let workspace = ModFixtureWorkspace::new("analyzer_multi_contracts");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.GenOne",  "entrySymbol": "gen_one" },
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.GenTwo",  "entrySymbol": "gen_two" },
    { "contractId": "Beskid.Compiler.Collect.Analyzer",  "typeId": "SampleMod.CheckOne", "entrySymbol": "check_one" },
    { "contractId": "Beskid.Compiler.Collect.Analyzer",  "typeId": "SampleMod.CheckTwo", "entrySymbol": "check_two" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();

    let invoker = ScriptedContractInvoker::new()
        .with_generator_contribution(
            "SampleMod.GenOne",
            vec!["pub fn from_gen_one() { return 1; }".to_owned()],
        )
        .with_generator_contribution(
            "SampleMod.GenTwo",
            vec!["pub fn from_gen_two() { return 2; }".to_owned()],
        )
        .with_analyzer_diagnostic(
            "SampleMod.CheckOne",
            vec![AnalyzerDiagnostic {
                code: "CHK001".to_owned(),
                message: "check one".to_owned(),
                severity: AnalyzerSeverity::Warning,
            }],
        )
        .with_analyzer_diagnostic(
            "SampleMod.CheckTwo",
            vec![AnalyzerDiagnostic {
                code: "CHK002".to_owned(),
                message: "check two".to_owned(),
                severity: AnalyzerSeverity::Warning,
            }],
        );

    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: Some(&invoker),
        },
    )
    .expect("generate");

    // Both generators produced outcomes with contributions.
    assert_eq!(generated.generator_outcomes.len(), 2);
    let gen_type_ids: Vec<&str> = generated
        .generator_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    assert!(gen_type_ids.contains(&"SampleMod.GenOne"));
    assert!(gen_type_ids.contains(&"SampleMod.GenTwo"));

    let gen_one = generated
        .generator_outcomes
        .iter()
        .find(|o| o.type_id == "SampleMod.GenOne")
        .expect("GenOne outcome");
    assert_eq!(gen_one.contributions.len(), 1);
    assert!(gen_one.contributions[0].contains("from_gen_one"));

    let gen_two = generated
        .generator_outcomes
        .iter()
        .find(|o| o.type_id == "SampleMod.GenTwo")
        .expect("GenTwo outcome");
    assert_eq!(gen_two.contributions.len(), 1);
    assert!(gen_two.contributions[0].contains("from_gen_two"));

    // Proceed through the semantic gate to analyze.
    let snapshot =
        beskid_analysis::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
            .with_composition(&generated.session.composition_snapshot_or_default());
    let analyze = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        Some(&invoker),
        Some(&snapshot),
        None,
    )
    .expect("analyze/rewrite");

    // Both analyzers dispatched, each with one diagnostic.
    assert_eq!(analyze.analyzer_outcomes.len(), 2);
    let check_ids: Vec<&str> = analyze
        .analyzer_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    assert!(check_ids.contains(&"SampleMod.CheckOne"));
    assert!(check_ids.contains(&"SampleMod.CheckTwo"));

    for outcome in &analyze.analyzer_outcomes {
        assert_eq!(
            outcome.diagnostics.len(),
            1,
            "each scripted analyzer must produce one diagnostic; type_id={}",
            outcome.type_id
        );
    }
}
