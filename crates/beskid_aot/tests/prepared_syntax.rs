use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_aot::lower_prepared_syntax_entrypoint;
use beskid_queries::compile_front_end_from_resolved_input;

#[test]
fn aot_lowers_prepared_syntax_with_its_object_target_isa() {
    let directory =
        std::env::temp_dir().join(format!("beskid_aot_prepared_syntax_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Main.bd");
    let source = "i32 Helper() { return 7; } i32 Main() { return Helper(); }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions {
            with_semantic_diagnostics: false,
            ..Default::default()
        },
        None,
    )
    .expect("prepare frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");

    let artifact = lower_prepared_syntax_entrypoint(&front, "Main", target)
        .expect("AOT prepared syntax lowering");
    assert_eq!(artifact.functions.len(), 2);
    std::fs::remove_dir_all(directory).expect("remove project");
}
