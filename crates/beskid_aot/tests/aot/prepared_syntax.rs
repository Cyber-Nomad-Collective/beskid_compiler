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
    assert_eq!(entry.function.signature.returns.len(), 1, "the issued entry is the free `i32 Main`, not the method");
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
/// (`native test must take no arguments and return void`).
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

    let mut object = beskid_aot::object_module::BeskidObjectModule::new(None, beskid_aot::BuildProfile::Debug)
        .expect("object module");
    object.compile_artifact(tests.artifact(), None).expect("test and function get distinct object symbols");
    object.finalize_to_path(&directory.path().join("tests.o")).expect("write test object");

    // The program entry still selects the function of that name.
    let program = lower_prepared_syntax_entrypoint(&front, "Main", host_target()).expect("program lowering");
    assert!(program.functions.iter().any(|function| function.name.starts_with("SumOfSquares#syntax_")));
}

#[test]
fn unobserved_automatic_property_has_the_field_layout_and_instructions() {
    let compile = |member: &str| {
        let directory = tempfile::tempdir().expect("project");
        let path = directory.path().join("Main.bd");
        let source = format!(
            "type Counter {{ i32 raw; {member} }}
i32 Main() {{
    Counter c = Counter {{ raw: 1, Count: 2 }};
    c.Count = c.raw + 3;
    return c.Count;
}}"
        );
        std::fs::write(&path, &source).expect("write source");
        let plan = synthetic_compile_plan_for_source(&path);
        let input = resolved_input_from_plan(path, source, plan, None, None);
        let front = compile_mod_free_front_end(&input, FrontEndOptions::default(), None).expect("prepare frontend");
        lower_prepared_syntax_entrypoint(&front, "Main", host_target()).expect("lower Main")
    };
    let field = compile("i32 Count;");
    let [field_plan] = field.aggregate_static_plans.as_slice() else { panic!("expected one Counter layout") };
    // Compare every instruction, including allocation and control flow. The CLIF header contains
    // compilation-specific symbol identities, so compare only the function body.
    let instructions = |artifact: &beskid_codegen::CodegenArtifact| {
        let [main] = artifact.functions.as_slice() else { panic!("expected only Main") };
        let clif = main.function.display().to_string();
        clif[clif.find("block0").expect("entry block")..].to_owned()
    };
    for member in ["prop i32 Count = 0;", "i32 Count { get; set; } = 0;"] {
        let property = compile(member);
        let [property_plan] = property.aggregate_static_plans.as_slice() else { panic!("expected one Counter layout") };
        assert_eq!(field_plan.object_size, property_plan.object_size);
        assert_eq!(field_plan.object_alignment, property_plan.object_alignment);
        assert_eq!(field_plan.pointer_map_offsets, property_plan.pointer_map_offsets);
        assert_eq!(field_plan.fields, property_plan.fields);
        assert_eq!(instructions(&field), instructions(&property));
    }
}

#[test]
fn observed_properties_share_one_traced_slot_without_allocating_during_construction() {
    let compile = |subscriptions: &str| {
        let directory = tempfile::tempdir().expect("project");
        let path = directory.path().join("Main.bd");
        let source = format!(
            "type Counter {{ prop i64 Count = 0_i64; prop i64 Limit = 0_i64; }}
Counter NewCounter() {{ return Counter {{}}; }}
i32 Main() {{
    Counter counter = NewCounter();
    {subscriptions}
    counter.Count = 1_i64;
    counter.Limit = 2_i64;
    return i32(counter.Count + counter.Limit);
}}"
        );
        std::fs::write(&path, &source).expect("write source");
        let plan = synthetic_compile_plan_for_source(&path);
        let input = resolved_input_from_plan(path, source, plan, None, None);
        let front = compile_mod_free_front_end(&input, FrontEndOptions::default(), None).expect("prepare frontend");
        lower_prepared_syntax_entrypoint(&front, "Main", host_target()).expect("lower Main")
    };
    let unobserved = compile("");
    let one = compile("observe counter.Count (old, new) => {};");
    let two = compile(
        "observe counter.Count (old, new) => {};
         observe counter.Limit (old, new) => {};",
    );

    let [base] = unobserved.aggregate_static_plans.as_slice() else { panic!("expected one Counter layout") };
    assert!(base.pointer_map_offsets.is_empty(), "Counter without observers has only scalar fields");
    let handle_size = u64::from(host_target().pointer_width) / 8;
    let constructor_instructions = |artifact: &beskid_codegen::CodegenArtifact| {
        let constructor = artifact
            .functions
            .iter()
            .find(|item| item.name.starts_with("NewCounter#"))
            .expect("constructor is reachable");
        let clif = constructor.function.display().to_string();
        assert!(!clif.contains("observer_"), "construction does not call observer services");
        clif[clif.find("block0").expect("entry block")..].to_owned()
    };
    let construction = constructor_instructions(&unobserved);
    for artifact in [&one, &two] {
        let [observed] = artifact.aggregate_static_plans.as_slice() else { panic!("expected one Counter layout") };
        assert_eq!(observed.object_size, base.object_size + handle_size);
        assert_eq!(observed.object_alignment, base.object_alignment);
        assert_eq!(observed.pointer_map_offsets.as_ref(), &[base.object_size]);
        assert_eq!(observed.fields, base.fields);
        assert_eq!(constructor_instructions(artifact), construction);
    }
}

#[test]
fn observation_only_instruments_the_subscribed_generic_specialization() {
    let compile = |subscription: &str| {
        let directory = tempfile::tempdir().expect("project");
        let path = directory.path().join("Main.bd");
        let source = format!(
            "type Box<T> {{ prop i32 Value = 0; }}
unit WriteTracked(Box<i32> box) {{ box.Value = 7; }}
unit WriteQuiet(Box<string> box) {{ box.Value = 9; }}
i32 Main() {{
    Box<i32> tracked = Box<i32> {{}};
    Box<string> quiet = Box<string> {{}};
    {subscription}
    WriteTracked(tracked);
    WriteQuiet(quiet);
    return tracked.Value + quiet.Value;
}}"
        );
        std::fs::write(&path, &source).expect("write source");
        let plan = synthetic_compile_plan_for_source(&path);
        let input = resolved_input_from_plan(path, source, plan, None, None);
        let front = compile_mod_free_front_end(&input, FrontEndOptions::default(), None).expect("prepare frontend");
        lower_prepared_syntax_entrypoint(&front, "Main", host_target()).expect("lower Main")
    };
    let baseline = compile("");
    let observed = compile("observe tracked.Value (old, new) => {};");
    let [first, second] = baseline.aggregate_static_plans.as_slice() else {
        panic!("expected two literals with the same physical field shapes")
    };
    assert_eq!(first.object_size, second.object_size);
    assert!(first.pointer_map_offsets.is_empty() && second.pointer_map_offsets.is_empty());
    let mut plans = observed.aggregate_static_plans.iter().collect::<Vec<_>>();
    plans.sort_by_key(|plan| plan.object_size);
    let [quiet, tracked] = plans.as_slice() else { panic!("expected two specialized literals") };
    assert_eq!(quiet.object_size, first.object_size);
    assert!(quiet.pointer_map_offsets.is_empty());
    assert_eq!(tracked.object_size, first.object_size + u64::from(host_target().pointer_width) / 8);
    assert_eq!(tracked.pointer_map_offsets.as_ref(), &[first.object_size]);

    let write_body = |artifact: &beskid_codegen::CodegenArtifact, name: &str| {
        let function = artifact.functions.iter().find(|item| item.name.starts_with(name)).expect("reachable writer");
        let clif = function.function.display().to_string();
        clif[clif.find("block0").expect("entry block")..].to_owned()
    };
    assert_eq!(write_body(&baseline, "WriteQuiet#"), write_body(&observed, "WriteQuiet#"));
    assert_ne!(write_body(&baseline, "WriteTracked#"), write_body(&observed, "WriteTracked#"));
}
