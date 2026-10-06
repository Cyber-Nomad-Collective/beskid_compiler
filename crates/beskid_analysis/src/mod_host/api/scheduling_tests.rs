//! Orchestration conformance for the post-`mod.load` scheduler: dispatch order, registration
//! conflicts, generator merge, analyzer coverage and replay stability.
//!
//! A unit-test binary never owns a qualified installed prefix or a built native Mod, so these
//! tests enter the scheduler with already-loaded registrations and a scripted or stub invoker.
//! Qualification of real executable artifacts is covered by the CLI native Mod tests and by
//! `descriptor.rs`/`load.rs` fail-closed tests.

use std::fs;

use anyhow::Result;

use super::tests::{MODA_MANIFEST, compile_plan, loaded_mod, unique_temp_dir, write_mod_projects};
use super::*;
use crate::mod_host::invoker::{
    AnalyzerDiagnostic, AnalyzerSeverity, ContractInvoker, ScriptedContractInvoker, StubContractInvoker,
};
use crate::syntax::Node;

const HOST_SOURCE: &str = "unit Main() { return; }\n";

const COLLECTOR: &str = "Beskid.Compiler.Collect.Collector";
const GENERATOR: &str = "Beskid.Compiler.Collect.Generator";
const ATTRIBUTE_GENERATOR: &str = "Beskid.Compiler.Collect.AttributeGenerator";
const ANALYZER: &str = "Beskid.Compiler.Collect.Analyzer";
const REWRITER: &str = "Beskid.Compiler.Collect.Rewriter";

const DEFAULT_REGISTRATIONS: [(&str, &str, &str); 5] = [
    (COLLECTOR, "SampleMod.SampleCollect", "samplemod_collect"),
    (GENERATOR, "SampleMod.SampleGenerate", "samplemod_generate"),
    (ATTRIBUTE_GENERATOR, "SampleMod.SampleAttribute", "samplemod_attribute"),
    (ANALYZER, "SampleMod.SampleAnalyze", "samplemod_analyze"),
    (REWRITER, "SampleMod.SampleRewrite", "samplemod_rewrite"),
];

struct Case {
    root: std::path::PathBuf,
    host: std::path::PathBuf,
    mod_dir: std::path::PathBuf,
}

impl Case {
    fn new(prefix: &str) -> Self {
        let root = unique_temp_dir(prefix);
        let (host, mod_dir) = write_mod_projects(&root, MODA_MANIFEST);
        Self { root, host, mod_dir }
    }

    fn generate(
        &self,
        registrations: &[(&str, &str, &str)],
        invoker: &dyn ContractInvoker,
    ) -> Result<ModHostGenerateResult> {
        let loaded = loaded_mod(&self.host, &self.mod_dir, registrations);
        let plan = compile_plan(&self.host, &self.mod_dir);
        let program = crate::services::parse_program_with_source_name("Main.bd", HOST_SOURCE).expect("parse host");
        schedule_generate(
            program,
            Vec::new(),
            &ModHostInput {
                semantic_scope: None,
                semantic_authority: None,
                compile_plan: Some(&plan),
                source_name: "Main.bd",
                source: HOST_SOURCE,
                pipeline: None,
                invoker: Some(invoker),
                cached_target_fingerprint: None,
                syntax_generation_id: None,
            },
            loaded,
            true,
        )
    }

    fn analyze(
        generated: ModHostGenerateResult,
        invoker: &dyn ContractInvoker,
    ) -> Result<crate::mod_host::ModHostAnalyzeResult> {
        let snapshot = crate::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
            .with_composition(&generated.session.composition_snapshot_or_default());
        run_analyze_rewrite_with_invoker(
            generated.program,
            &generated.session,
            Some(invoker),
            None,
            Some(&snapshot),
            None,
        )
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn warning(code: &str, message: &str) -> Vec<AnalyzerDiagnostic> {
    vec![AnalyzerDiagnostic {
        code: code.to_owned(),
        message: message.to_owned(),
        severity: AnalyzerSeverity::Warning,
        span: None,
    }]
}

fn is_function(node: &Node, name: &str) -> bool {
    matches!(node, Node::Function(function) if function.node.name.node.name == name)
}

fn program_has_function(program: &crate::syntax::Spanned<crate::syntax::Program>, name: &str) -> bool {
    program.node.items.iter().any(|item| is_function(&item.node, name))
}

fn conflict_codes(registrations: &[(&str, &str, &str)], prefix: &str) -> Vec<String> {
    let case = Case::new(prefix);
    let invoker = StubContractInvoker::new();
    let error = match case.generate(registrations, &invoker) {
        Ok(_) => panic!("conflicting registrations must abort scheduling"),
        Err(error) => error,
    };
    assert!(invoker.invocations().is_empty(), "conflicts abort before any dispatch");
    extract_mod_host_diagnostics(&error)
        .expect("mod host diagnostics surfaced through anyhow chain")
        .codes()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn unknown_contract_id_emits_e1853() {
    let codes = conflict_codes(
        &[("Beskid.Compiler.Made.Up", "SampleMod.SampleGenerate", "samplemod_generate")],
        "sched_unknown_e1853",
    );
    assert!(codes.iter().any(|code| code == "E1853"), "{codes:?}");
}

#[test]
fn rewriter_without_analyzer_emits_e1854() {
    let codes = conflict_codes(&[(REWRITER, "SampleMod.SampleRewrite", "samplemod_rewrite")], "sched_rewriter_e1854");
    assert!(codes.iter().any(|code| code == "E1854"), "{codes:?}");
}

#[test]
fn missing_entry_symbol_emits_e1828() {
    let codes = conflict_codes(&[(GENERATOR, "SampleMod.SampleGenerate", "")], "sched_missing_entry_e1828");
    assert!(codes.iter().any(|code| code == "E1828"), "{codes:?}");
}

#[test]
fn duplicate_registration_in_one_artifact_emits_e1829() {
    let codes = conflict_codes(
        &[
            (GENERATOR, "SampleMod.SampleGenerate", "samplemod_generate"),
            (GENERATOR, "SampleMod.SampleGenerate", "samplemod_generate_dup"),
        ],
        "sched_duplicate_e1829",
    );
    assert!(codes.iter().any(|code| code == "E1829"), "{codes:?}");
}

#[test]
fn scripted_invoker_surfaces_generator_items_and_analyzer_diagnostics_in_dispatch_order() {
    let case = Case::new("sched_scripted_dispatch");
    let invoker = ScriptedContractInvoker::new()
        .with_analyzer_diagnostic("SampleMod.SampleAnalyze", warning("SampleMod0001", "synthetic analyzer diagnostic"))
        .with_generator_contribution(
            "SampleMod.SampleGenerate",
            vec!["pub fn sample_synthetic_marker() { return; }".to_owned()],
        );
    let generated = case.generate(&DEFAULT_REGISTRATIONS, &invoker).expect("generate");
    assert_eq!(generated.session.loaded_descriptor_count(), 1);
    assert_eq!(generated.collector_outcomes.len(), 1);
    assert_eq!(generated.collector_outcomes[0].type_id, "SampleMod.SampleCollect");
    let generator_ids: Vec<&str> =
        generated.generator_outcomes.iter().map(|outcome| outcome.type_id.as_str()).collect();
    assert_eq!(generator_ids.len(), 2);
    assert!(generator_ids.contains(&"SampleMod.SampleGenerate"));
    assert!(generator_ids.contains(&"SampleMod.SampleAttribute"));
    let synthetic = generated
        .generator_outcomes
        .iter()
        .find(|outcome| outcome.type_id == "SampleMod.SampleGenerate")
        .expect("scripted generator outcome");
    assert_eq!(synthetic.typed_items.len(), 1);
    assert!(is_function(&synthetic.typed_items[0].node, "sample_synthetic_marker"));

    let analyze = Case::analyze(generated, &invoker).expect("analyze rewrite");
    let analyzer = analyze
        .analyzer_outcomes
        .iter()
        .find(|outcome| outcome.type_id == "SampleMod.SampleAnalyze")
        .expect("scripted analyzer outcome");
    assert_eq!(analyzer.diagnostics.len(), 1);
    assert_eq!(analyzer.diagnostics[0].code, "SampleMod0001");
    assert_eq!(analyzer.diagnostics[0].severity, AnalyzerSeverity::Warning);
    assert_eq!(analyze.rewriter_outcomes.len(), 1);
    assert_eq!(analyze.rewriter_outcomes[0].type_id, "SampleMod.SampleRewrite");
}

#[test]
fn typed_generator_items_merge_into_host_program() {
    let case = Case::new("sched_typed_merge");
    let invoker = ScriptedContractInvoker::new().with_generator_contribution(
        "SampleMod.SampleGenerate",
        vec!["pub fn typed_merge_marker() { return; }".to_owned()],
    );
    let generated =
        case.generate(&[(GENERATOR, "SampleMod.SampleGenerate", "samplemod_generate")], &invoker).expect("generate");
    assert_eq!(generated.generator_outcomes.len(), 1);
    assert!(
        generated.generator_outcomes[0].typed_items.iter().any(|item| is_function(&item.node, "typed_merge_marker"))
    );
    assert!(program_has_function(&generated.program, "typed_merge_marker"), "host program must include merged item");
}

#[test]
fn multiple_generators_and_analyzers_dispatch_in_order() {
    let case = Case::new("sched_multi_contracts");
    let invoker = ScriptedContractInvoker::new()
        .with_generator_contribution("SampleMod.GenOne", vec!["pub fn from_gen_one() { return 1; }".to_owned()])
        .with_generator_contribution("SampleMod.GenTwo", vec!["pub fn from_gen_two() { return 2; }".to_owned()])
        .with_analyzer_diagnostic("SampleMod.CheckOne", warning("CHK001", "check one"))
        .with_analyzer_diagnostic("SampleMod.CheckTwo", warning("CHK002", "check two"));
    let generated = case
        .generate(
            &[
                (GENERATOR, "SampleMod.GenOne", "gen_one"),
                (GENERATOR, "SampleMod.GenTwo", "gen_two"),
                (ANALYZER, "SampleMod.CheckOne", "check_one"),
                (ANALYZER, "SampleMod.CheckTwo", "check_two"),
            ],
            &invoker,
        )
        .expect("generate");
    assert_eq!(generated.generator_outcomes.len(), 2);
    for (type_id, function) in [("SampleMod.GenOne", "from_gen_one"), ("SampleMod.GenTwo", "from_gen_two")] {
        let outcome = generated.generator_outcomes.iter().find(|o| o.type_id == type_id).expect("generator outcome");
        assert_eq!(outcome.typed_items.len(), 1);
        assert!(is_function(&outcome.typed_items[0].node, function));
    }
    let analyze = Case::analyze(generated, &invoker).expect("analyze rewrite");
    let ids: Vec<&str> = analyze.analyzer_outcomes.iter().map(|outcome| outcome.type_id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"SampleMod.CheckOne") && ids.contains(&"SampleMod.CheckTwo"));
    assert!(analyze.analyzer_outcomes.iter().all(|outcome| outcome.diagnostics.len() == 1));
}

#[test]
fn generator_contributions_surface_in_outcomes_and_analyzer_dispatches_afterwards() {
    let case = Case::new("sched_analyzer_after_generator");
    let invoker = ScriptedContractInvoker::new()
        .with_generator_contribution(
            "SampleMod.SampleGenerate",
            vec!["pub fn generated_func() { return 42; }".to_owned()],
        )
        .with_analyzer_diagnostic("SampleMod.SampleAnalyze", warning("COV0001", "analyzer processed program"));
    let generated = case
        .generate(
            &[
                (GENERATOR, "SampleMod.SampleGenerate", "samplemod_generate"),
                (ANALYZER, "SampleMod.SampleAnalyze", "samplemod_analyze"),
                (REWRITER, "SampleMod.SampleRewrite", "samplemod_rewrite"),
            ],
            &invoker,
        )
        .expect("generate with contributions");
    assert_eq!(generated.generator_outcomes.len(), 1);
    assert!(program_has_function(&generated.program, "generated_func"));
    let analyze = Case::analyze(generated, &invoker).expect("analyze rewrite");
    assert_eq!(analyze.analyzer_outcomes.len(), 1);
    assert_eq!(analyze.analyzer_outcomes[0].diagnostics[0].code, "COV0001");
    assert_eq!(analyze.rewriter_outcomes.len(), 1);
    assert_eq!(analyze.rewriter_outcomes[0].type_id, "SampleMod.SampleRewrite");
}

#[test]
fn identical_registrations_produce_stable_outcomes_and_changed_registrations_do_not() {
    let case = Case::new("sched_replay_stability");
    let invoker = StubContractInvoker::new();
    let first = case.generate(&DEFAULT_REGISTRATIONS, &invoker).expect("first run");
    let second = case.generate(&DEFAULT_REGISTRATIONS, &invoker).expect("second run");
    let ids = |result: &ModHostGenerateResult| {
        (
            result.collector_outcomes.iter().map(|o| o.type_id.clone()).collect::<Vec<_>>(),
            result.generator_outcomes.iter().map(|o| o.type_id.clone()).collect::<Vec<_>>(),
            result.session.loaded_descriptor_count(),
        )
    };
    assert_eq!(ids(&first), ids(&second), "identical inputs must replay identically");

    let reduced = case
        .generate(&[(GENERATOR, "SampleMod.SampleGenerate", "samplemod_generate")], &invoker)
        .expect("reduced registrations");
    assert_ne!(first.collector_outcomes.len(), reduced.collector_outcomes.len());
    assert_ne!(first.generator_outcomes.len(), reduced.generator_outcomes.len());
}
