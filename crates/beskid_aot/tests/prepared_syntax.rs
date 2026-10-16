use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_aot::lower_prepared_syntax_entrypoint;
use beskid_queries::compile_mod_free_front_end;

#[test]
fn aot_lowers_prepared_syntax_with_its_object_target_isa() {
    let directory = std::env::temp_dir().join(format!("beskid_aot_prepared_syntax_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Main.bd");
    let source = "i32 Helper() { return 7; } i32 Main() { return Helper(); }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_mod_free_front_end(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .expect("prepare frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");

    let artifact = lower_prepared_syntax_entrypoint(&front, "Main", target).expect("AOT prepared syntax lowering");
    assert_eq!(artifact.functions.len(), 2);
    std::fs::remove_dir_all(directory).expect("remove project");
}

#[test]
fn entry_identity_is_the_free_function_even_when_a_type_method_shares_its_name() {
    let directory = std::env::temp_dir().join(format!("beskid_aot_same_name_entry_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create project");
    let path = directory.join("Main.bd");
    // The type and its `Main` method precede the program entry and have a different result type.
    let source = "type Other { i64 value, i64 Main() { return this.value; } } \
                  i32 Main() { Other other = Other { value: 3_i64 }; i64 seen = other.Main(); return 0; }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_mod_free_front_end(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .expect("prepare frontend");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str().starts_with("x86_64-"))
        .expect("x86_64 ABI target");

    let artifact = lower_prepared_syntax_entrypoint(&front, "Main", target).expect("AOT prepared syntax lowering");
    let mains = artifact.functions.iter().filter(|function| function.name.starts_with("Main#")).collect::<Vec<_>>();
    assert_eq!(mains.len(), 2, "both callables named Main are emitted: {mains:?}");
    let entry_name = artifact.executable_entry.as_deref().expect("the lowering issues the entry identity");
    let entry = artifact.functions.iter().find(|function| function.name == entry_name).expect("entry is emitted");
    assert_eq!(
        entry.function.signature.returns.len(),
        1,
        "the issued entry is the free `i32 Main`, not the method"
    );
    assert_eq!(entry.function.signature.returns[0].value_type, cranelift_codegen::ir::types::I32);
    assert_eq!(
        mains.iter().filter(|function| function.name == entry_name).count(),
        1,
        "the entry identity is unique among same-named callables"
    );
    std::fs::remove_dir_all(directory).expect("remove project");
}
