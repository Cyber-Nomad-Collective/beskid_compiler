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
    let source = "type Other { i64 value; i64 Main() { return this.value; } } \
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

fn host_target() -> TargetMetadata {
    let triple = if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_arch = "x86_64", target_os = "windows")) {
        "x86_64-pc-windows-msvc"
    } else {
        panic!("unsupported test host");
    }
    .to_owned();
    TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == triple).expect("host ABI target")
}

/// Tests have their own namespace (owner ruling, 0.6.7): `test SumOfSquares` next to
/// `i32 SumOfSquares(i32 n)` passes `beskid check`, and its native test build must select the test,
/// not the function. Selecting by name alone picked the function and the build failed with E4041
/// (`native test must take no arguments and return void`), seen in the 0.6.7-preview.2 smoke.
#[test]
fn a_test_sharing_its_name_with_a_function_builds_as_a_native_test_object() {
    let directory = tempfile::tempdir().expect("project");
    let path = directory.path().join("Main.bd");
    let source = "i32 SumOfSquares(i32 n) { return n * n; }\n\
                  test SumOfSquares { i32 value = SumOfSquares(2); }\n\
                  i32 Main() { return SumOfSquares(3); }";
    std::fs::write(&path, source).expect("write source");
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved: ResolvedInput = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_mod_free_front_end(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        None,
    )
    .expect("prepare frontend");

    let tests = beskid_aot::prepared_syntax::lower_frozen_syntax_entrypoints(
        &front,
        &["SumOfSquares".to_owned()],
        host_target(),
    )
    .expect("the test, not the same-named function, is the selected native test");
    let [entry] = tests.entries() else { panic!("one selected test") };
    assert_eq!(entry.return_type(), beskid_queries::SemanticTypeId::UNIT, "the selected entry is the test");
    let named = tests
        .artifact()
        .functions
        .iter()
        .filter(|function| function.name.starts_with("SumOfSquares#syntax_"))
        .collect::<Vec<_>>();
    assert_eq!(named.len(), 2, "the test and the function it calls are two emitted functions: {named:?}");
    let test = named.iter().find(|function| function.name == entry.symbol()).expect("the test is emitted");
    assert!(test.function.signature.params.is_empty(), "the test entry takes no arguments");

    let mut object =
        beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug).expect("object module");
    object.compile_artifact(tests.artifact(), None).expect("test and function get distinct object symbols");
    object.finalize_to_path(&directory.path().join("tests.o")).expect("write test object");

    // The program entry still selects the function of that name.
    let program = lower_prepared_syntax_entrypoint(&front, "Main", host_target()).expect("program lowering");
    assert!(program.functions.iter().any(|function| function.name.starts_with("SumOfSquares#syntax_")));
}
