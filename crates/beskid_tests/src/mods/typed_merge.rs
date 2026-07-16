//! Typed AST merge conformance: generator outcomes splice into host `Program`.

use beskid_analysis::mod_host::{ModHostInput, ScriptedContractInvoker, run_through_generate};
use beskid_analysis::services::parse_program_with_source_name;

use super::fixture::{
    ModFixtureWorkspace, program_contains_function, typed_items_contain_function,
};

#[test]
fn typed_generator_items_merge_into_host_program() {
    let workspace = ModFixtureWorkspace::new("typed_merge_host_program");
    workspace.write_descriptor(
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Generator", "typeId": "SampleMod.SampleGenerate", "entrySymbol": "samplemod_generate" }
  ]"#,
    );

    let source = workspace.host_source();
    let plan = workspace.compile_plan();
    let invoker = ScriptedContractInvoker::new().with_generator_contribution(
        "SampleMod.SampleGenerate",
        vec!["pub fn typed_merge_marker() { return; }".to_owned()],
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
            cached_target_fingerprint: None,
        },
    )
    .expect("typed merge generate");

    assert_eq!(generated.generator_outcomes.len(), 1);
    assert!(typed_items_contain_function(
        &generated.generator_outcomes[0].typed_items,
        "typed_merge_marker"
    ));
    assert!(
        program_contains_function(&generated.program, "typed_merge_marker"),
        "host program must include merged typed item"
    );
}
