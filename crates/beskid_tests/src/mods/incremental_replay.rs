//! Incremental replay / deterministic schedule conformance tests.
//!
//! Per the platform-spec conformance evidence policy (evidence category 3 —
//! Incremental replay), tests must demonstrate that the mod host produces
//! **stable generator outputs** when key tuples match, and that changing an
//! input (descriptor registrations, capability set, source hash) causes
//! cache-miss behaviour with distinct outputs.

use beskid_analysis::mod_host::{ModHostInput, run_through_generate};
use beskid_analysis::services::parse_program_with_source_name;

use super::fixture::ModFixtureWorkspace;

#[test]
fn duplicate_identical_descriptor_produces_stable_collector_and_generator_outcomes() {
    let workspace = ModFixtureWorkspace::new("incremental_stable_outcomes");
    workspace.write_descriptor(ModFixtureWorkspace::default_registrations_json());

    let source = workspace.host_source();
    let plan = workspace.compile_plan();

    let program_a = parse_program_with_source_name("Main.bd", source).expect("parse host");
    let result_a = run_through_generate(
        program_a,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    )
    .expect("first generate run");

    // Second run with identical inputs — asserts deterministic outcomes.
    let program_b = parse_program_with_source_name("Main.bd", source).expect("parse host again");
    let result_b = run_through_generate(
        program_b,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    )
    .expect("second generate run");

    assert_eq!(
        result_a.collector_outcomes.len(),
        result_b.collector_outcomes.len(),
        "collector outcome count must be stable"
    );
    assert_eq!(
        result_a.generator_outcomes.len(),
        result_b.generator_outcomes.len(),
        "generator outcome count must be stable"
    );
    assert_eq!(
        result_a.session.loaded_descriptor_count(),
        result_b.session.loaded_descriptor_count(),
        "descriptor count must be stable"
    );

    // Compare collector type_ids across runs.
    let collect_a: Vec<&str> = result_a
        .collector_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    let collect_b: Vec<&str> = result_b
        .collector_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    assert_eq!(
        collect_a, collect_b,
        "collector type_ids must be identical across identical inputs"
    );

    // Compare generator type_ids across runs.
    let gen_a: Vec<&str> = result_a
        .generator_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    let gen_b: Vec<&str> = result_b
        .generator_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    assert_eq!(
        gen_a, gen_b,
        "generator type_ids must be identical across identical inputs"
    );
}

#[test]
fn changed_registration_produces_different_outcome_count() {
    let workspace = ModFixtureWorkspace::new("incremental_changed_registration");

    // First run with default registrations (5 contracts).
    workspace.write_descriptor(ModFixtureWorkspace::default_registrations_json());

    let source = workspace.host_source();
    let plan = workspace.compile_plan();

    let program_a = parse_program_with_source_name("Main.bd", source).expect("parse");
    let result_a = run_through_generate(
        program_a,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    )
    .expect("first run");

    // Second run with only a Generator registration (different from default which has 5 contracts).
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" }
  ]"#,
    );

    let program_b = parse_program_with_source_name("Main.bd", source).expect("parse again");
    let result_b = run_through_generate(
        program_b,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    )
    .expect("second run with reduced registrations");

    // Different registration sets must produce different outcome counts.
    assert_ne!(
        result_a.collector_outcomes.len(),
        result_b.collector_outcomes.len(),
        "changed registrations must change collector outcomes"
    );
    assert_ne!(
        result_a.generator_outcomes.len(),
        result_b.generator_outcomes.len(),
        "changed registrations must change generator outcomes"
    );
}

#[test]
fn scripted_generator_contributions_are_stable_across_identical_runs() {
    let workspace = ModFixtureWorkspace::new("incremental_scripted_stable");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();

    let program_a = parse_program_with_source_name("Main.bd", source).expect("parse");
    let result_a = run_through_generate(
        program_a,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    )
    .expect("first scripted run");

    let program_b = parse_program_with_source_name("Main.bd", source).expect("parse again");
    let result_b = run_through_generate(
        program_b,
        &ModHostInput {
            compile_plan: Some(&plan),
            source_name: "Main.bd",
            source,
            pipeline: None,
            invoker: None,
            cached_target_fingerprint: None,
        },
    )
    .expect("second scripted run");

    // Generator type_ids and contribution counts must be identical.
    let gen_ids_a: Vec<&str> = result_a
        .generator_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    let gen_ids_b: Vec<&str> = result_b
        .generator_outcomes
        .iter()
        .map(|o| o.type_id.as_str())
        .collect();
    assert_eq!(
        gen_ids_a, gen_ids_b,
        "generator type_ids must be identical for identical regs"
    );

    // Typed items from stub generators are empty; verify stability of that emptiness.
    for (outcome_a, outcome_b) in result_a
        .generator_outcomes
        .iter()
        .zip(result_b.generator_outcomes.iter())
    {
        assert_eq!(
            outcome_a.typed_items.len(),
            outcome_b.typed_items.len(),
            "typed item count for {} must be stable",
            outcome_a.type_id
        );
    }
}
