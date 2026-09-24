//! Spawn arguments and lambda spawn entries with and without captures.

use super::*;

#[test]
fn parsed_direct_spawn_with_arguments_transfers_them_through_a_traced_start_environment() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        type Counter { i64 value, }
        i64 Entry(i64 delta, Counter counter) { return counter.value + delta; }
        i64 Main() {
            Counter counter = Counter { value: 40_i64 };
            let child = spawn Entry(2_i64, counter);
            return 0_i64;
        }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    let plan = lowered
        .artifact
        .aggregate_static_plans
        .iter()
        .find(|plan| plan.allocation_request_symbol.starts_with("__beskid_spawn_arguments_request_"))
        .expect("argument-bearing spawn must materialize a start-environment plan");
    assert_eq!(plan.fields.len(), 2, "one environment field per entry parameter");
    assert_eq!(plan.pointer_map_offsets.len(), 1, "the managed Counter argument is traced");
    assert_eq!(plan.pointer_map_offsets[0], plan.fields[1].field_offset);

    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains(&plan.allocation_request_symbol), "{main_clif}");
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(main_clif.contains("gc_register_root"), "{main_clif}");
    assert!(main_clif.contains("gc_unregister_root"), "{main_clif}");
    assert!(
        !main_clif.contains("Entry#syntax_"),
        "the parent evaluates arguments but never invokes the entry: {main_clif}"
    );
    assert!(trampoline_clif.contains("Entry#syntax_"), "{trampoline_clif}");
    let entry = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Entry#syntax_"))
        .expect("spawned entry is reachable");
    let target_signature = trampoline
        .function
        .dfg
        .ext_funcs
        .values()
        .find(|function| function.name == ExternalName::testcase(entry.name.as_bytes()))
        .map(|function| &trampoline.function.dfg.signatures[function.signature])
        .expect("trampoline invokes the source-owned entry");
    assert_eq!(target_signature.params.len(), 2, "the trampoline passes both loaded arguments");
    let loads = trampoline
        .function
        .layout
        .blocks()
        .flat_map(|block| trampoline.function.layout.block_insts(block))
        .filter(|inst| trampoline.function.dfg.insts[*inst].opcode() == cranelift_codegen::ir::Opcode::Load)
        .count();
    assert_eq!(loads, 2, "the trampoline loads each argument from the environment\n{trampoline_clif}");
}

#[test]
fn parsed_lambda_spawn_with_call_arguments_fails_closed() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        i64 Main() { let child = spawn ((i64 value) => value)(7_i64); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Unsupported.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    assert_unsupported_closed_failure(assembly, target, isa.as_ref(), &["CalleeArgumentsUnsupported"]);
}

#[test]
fn parsed_zero_capture_lambda_spawn_emits_syntax_owned_entry_and_fiber_dispatch() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        i64 Main() { let child = spawn (() => 7_i64); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(
        lowered.artifact.functions.len(),
        4,
        "Main, the lambda body entry, the syntax-owned spawn entry, and its trampoline: {:?}",
        lowered.artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
    );
    assert!(
        lowered.artifact.functions.iter().any(|function| function.name.starts_with("__beskid_lambda_entry_syntax_")),
        "lambda body entry must remain part of the reachable closure"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let lambda = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("syntax-owned lambda entry");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let lambda_clif = lambda.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(main_clif.contains("func_addr"), "{main_clif}");
    assert!(lambda_clif.contains("iconst.i64 7"), "{lambda_clif}");
    assert!(trampoline_clif.contains("__beskid_spawn_lambda_syntax_"), "{trampoline_clif}");
}

#[test]
fn parsed_capturing_lambda_spawn_allocates_roots_and_dispatches_fiber_entry() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        i64 Main() { i64 outer = 41_i64; let child = spawn (() => outer); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();

    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert!(
        !lowered.artifact.closure_static_plans.is_empty(),
        "capturing spawn must materialize a generation-safe closure static plan"
    );
    assert_eq!(
        lowered.artifact.closure_static_plans[0].captures.len(),
        1,
        "outer capture must appear in the static plan"
    );
    let main = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Main#syntax_"))
        .expect("Main artifact");
    let lambda = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("syntax-owned capturing lambda entry");
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("syntax-owned spawn trampoline");
    let main_clif = main.function.display().to_string();
    let lambda_clif = lambda.function.display().to_string();
    let trampoline_clif = trampoline.function.display().to_string();
    assert!(main_clif.contains("beskid_rt_v5_closure_environment_allocate"), "{main_clif}");
    assert!(main_clif.contains("gc_register_root"), "{main_clif}");
    assert!(main_clif.contains("gc_unregister_root"), "{main_clif}");
    assert!(!main_clif.contains("beskid_rt_v5_closure_environment_root_current"), "{main_clif}");
    assert!(main_clif.contains("fiber_spawn"), "{main_clif}");
    assert!(
        lambda_clif.contains("load") || lambda_clif.contains("ireduce") || lambda_clif.contains("iadd"),
        "lambda entry must read the rooted capture environment: {lambda_clif}"
    );
    assert!(trampoline_clif.contains("__beskid_spawn_lambda_syntax_"), "{trampoline_clif}");
}

#[test]
fn parsed_unit_capturing_lambda_spawn_preserves_zero_return_entry() {
    let project = tempfile::tempdir().expect("project directory");
    let source = "
        pub type Fiber<T> { i64 handle, }
        unit Serve(i64 listener) { return; }
        i64 Main() { i64 listener = 41_i64; let child = spawn (() => Serve(listener)); return 0_i64; }
    ";
    let assembly = parse_production_units(project.path(), &[("Main.bd", "Main", source)]);
    let (target, isa) = x86_64_target_and_isa();
    let lowered = lower_verified_entrypoint(assembly, target, isa.as_ref());
    assert_eq!(lowered.artifact.closure_static_plans[0].captures.len(), 1);
    for lambda in lowered.artifact.functions.iter().filter(|function| {
        function.name.starts_with("__beskid_spawn_lambda_syntax_")
            || function.name.starts_with("__beskid_lambda_entry_syntax_")
    }) {
        assert_eq!(lambda.function.signature.params.len(), 1, "capture environment");
        assert!(lambda.function.signature.returns.is_empty(), "unit has no ABI return register");
        let clif = lambda.function.display().to_string();
        assert!(clif.contains("Serve#syntax_"), "unit body keeps its effect: {clif}");
        assert!(clif.contains("load.i64"), "body reads its capture: {clif}");
    }
    let trampoline = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_entry_syntax_"))
        .expect("spawn trampoline");
    assert_eq!(trampoline.function.signature.returns.len(), 1, "transport still returns its owned result box");
    let entry = lowered
        .artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("__beskid_spawn_lambda_syntax_"))
        .expect("spawned lambda entry");
    let target_signature = trampoline
        .function
        .dfg
        .ext_funcs
        .values()
        .find(|function| function.name == ExternalName::testcase(entry.name.as_bytes()))
        .map(|function| &trampoline.function.dfg.signatures[function.signature])
        .expect("trampoline invokes the source-owned entry");
    assert!(target_signature.returns.is_empty());
}
