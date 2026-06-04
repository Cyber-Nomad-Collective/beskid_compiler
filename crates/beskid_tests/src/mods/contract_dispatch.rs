//! End-to-end dispatch through `mod.collect`/`generate`/`analyze`/`rewrite`.
//!
//! These tests assert that the host pipeline:
//! - Loads the AOT descriptor sidecar from the fixture.
//! - Dispatches each scheduled `(contractId, typeId)` exactly once via
//!   [`beskid_analysis::mod_host::ContractInvoker`].
//! - Reports per-contract outcomes back to callers (collector / generator
//!   outcomes from `run_through_generate`; analyzer / rewriter outcomes from
//!   `run_analyze_rewrite_with_invoker`).
//! - Emits pipeline phases in the canonical order from
//!   `beskid_pipeline::phases::FULL_BUILD_PHASE_ORDER`.

use std::sync::Mutex;

use beskid_analysis::mod_host::{
    AnalyzerDiagnostic, AnalyzerSeverity, InvocationKind, ModHostInput, ScriptedContractInvoker,
    StubContractInvoker, run_analyze_rewrite_with_invoker, run_through_generate,
};
use beskid_analysis::services::parse_program_with_source_name;
use beskid_pipeline::phases::{
    MACRO_EXPAND, MOD_ANALYZE, MOD_COLLECT, MOD_GENERATE, MOD_LOAD, MOD_REWRITE,
};
use beskid_pipeline::{PipelineEvent, PipelineObserver};

use super::fixture::ModFixtureWorkspace;

#[derive(Default)]
struct CapturePipeline {
    phase_starts: Mutex<Vec<&'static str>>,
}

impl CapturePipeline {
    fn phase_starts(&self) -> Vec<&'static str> {
        self.phase_starts.lock().expect("phase starts").clone()
    }
}

impl PipelineObserver for CapturePipeline {
    fn on_event(&self, event: PipelineEvent) {
        if let PipelineEvent::PhaseStart { id } = event {
            self.phase_starts.lock().expect("phase starts").push(id);
        }
    }
}

#[test]
fn sample_mod_dispatches_all_four_contract_kinds_through_invoker() {
    let workspace = ModFixtureWorkspace::new("sample_mod_dispatch_all_four");
    workspace.write_descriptor(ModFixtureWorkspace::default_registrations_json());

    let source = workspace.host_source();
    let plan = workspace.compile_plan();
    let pipeline = CapturePipeline::default();
    let invoker = StubContractInvoker::new();

    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: Some(&pipeline),
            invoker: Some(&invoker),
        },
    )
    .expect("mod host generate");

    assert_eq!(generated.session.loaded_descriptor_count(), 1);
    assert_eq!(generated.collector_outcomes.len(), 1);
    assert_eq!(generated.collector_outcomes[0].type_id, "SampleMod.SampleCollect");
    assert_eq!(generated.generator_outcomes.len(), 2);
    let generator_type_ids: Vec<&str> = generated
        .generator_outcomes
        .iter()
        .map(|outcome| outcome.type_id.as_str())
        .collect();
    assert!(generator_type_ids.contains(&"SampleMod.SampleGenerate"));
    assert!(generator_type_ids.contains(&"SampleMod.SampleAttribute"));

    let snapshot = beskid_analysis::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
        .with_composition(&generated.session.composition_snapshot_or_default());
    let analyze = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        Some(&invoker),
        Some(&snapshot),
        Some(&pipeline),
    )
    .expect("mod host analyze rewrite");

    assert_eq!(analyze.analyzer_outcomes.len(), 1);
    assert_eq!(analyze.analyzer_outcomes[0].type_id, "SampleMod.SampleAnalyze");
    assert_eq!(analyze.rewriter_outcomes.len(), 1);
    assert_eq!(analyze.rewriter_outcomes[0].type_id, "SampleMod.SampleRewrite");

    let invocations = invoker.invocations();
    assert_eq!(
        invocations.len(),
        5,
        "collector + generator + attribute generator + analyzer + rewriter"
    );
    let kinds: Vec<&str> = invocations
        .iter()
        .map(|inv| match inv {
            InvocationKind::Collector { .. } => "collector",
            InvocationKind::Generator { .. } => "generator",
            InvocationKind::Analyzer { .. } => "analyzer",
            InvocationKind::Rewriter { .. } => "rewriter",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["collector", "generator", "generator", "analyzer", "rewriter"],
        "host must dispatch contracts in canonical order"
    );

    let events = pipeline.phase_starts();
    assert_subsequence(
        &events,
        &[
            MACRO_EXPAND,
            MOD_LOAD,
            MOD_COLLECT,
            MOD_GENERATE,
            MACRO_EXPAND,
            MOD_ANALYZE,
            MOD_REWRITE,
        ],
    );
}

#[test]
fn scripted_invoker_surfaces_analyzer_diagnostics_to_outcomes() {
    let workspace = ModFixtureWorkspace::new("sample_mod_scripted_diagnostics");
    workspace.write_descriptor(ModFixtureWorkspace::default_registrations_json());

    let source = workspace.host_source();
    let plan = workspace.compile_plan();
    let pipeline = CapturePipeline::default();
    let invoker = ScriptedContractInvoker::new()
        .with_analyzer_diagnostic(
            "SampleMod.SampleAnalyze",
            vec![AnalyzerDiagnostic {
                code: "SampleMod0001".to_owned(),
                message: "synthetic analyzer diagnostic from SampleMod".to_owned(),
                severity: AnalyzerSeverity::Warning,
            }],
        )
        .with_generator_contribution(
            "SampleMod.SampleGenerate",
            vec!["pub fn sample_synthetic_marker() { return; }".to_owned()],
        );

    let program = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let generated = run_through_generate(
        program,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: Some(&pipeline),
            invoker: Some(&invoker),
        },
    )
    .expect("mod host generate");
    let synthetic = generated
        .generator_outcomes
        .iter()
        .find(|outcome| outcome.type_id == "SampleMod.SampleGenerate")
        .expect("scripted generator outcome");
    assert_eq!(synthetic.contributions.len(), 1);
    assert!(
        synthetic.contributions[0].contains("sample_synthetic_marker"),
        "scripted generator contribution should be propagated to outcome"
    );

    let snapshot = beskid_analysis::services::SemanticSnapshot::from_diagnostics(&[], 1, "semantic")
        .with_composition(&generated.session.composition_snapshot_or_default());
    let analyze = run_analyze_rewrite_with_invoker(
        generated.program,
        &generated.session,
        Some(&invoker),
        Some(&snapshot),
        Some(&pipeline),
    )
    .expect("mod host analyze rewrite");

    let analyzer = analyze
        .analyzer_outcomes
        .iter()
        .find(|outcome| outcome.type_id == "SampleMod.SampleAnalyze")
        .expect("scripted analyzer outcome");
    assert_eq!(analyzer.diagnostics.len(), 1);
    assert_eq!(analyzer.diagnostics[0].code, "SampleMod0001");
    assert_eq!(analyzer.diagnostics[0].severity, AnalyzerSeverity::Warning);
}

fn assert_subsequence(events: &[&'static str], expected: &[&'static str]) {
    let mut cursor = 0usize;
    for event in events {
        if cursor < expected.len() && *event == expected[cursor] {
            cursor += 1;
        }
    }
    assert_eq!(
        cursor,
        expected.len(),
        "expected phase subsequence {expected:?} in observed events {events:?}"
    );
}
