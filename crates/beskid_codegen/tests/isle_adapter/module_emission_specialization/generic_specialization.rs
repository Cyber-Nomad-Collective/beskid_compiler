//! Call-derived generic specializations, rejections, and specialized layouts.

use super::super::support::{
    SyntaxModuleItem, find_function_definitions, find_node, item_fixture_with_root, item_name, lower_syntax_program,
};
use super::{StringComparison, assert_string_content_comparison};

#[test]
fn generic_enum_constructor_uses_mutable_local_assignment_context() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Option<T> { Some(T value), None } Option<T> Last<T>(T value) { mut Option<T> last = Option::None(); last = Option::Some(value); return last; } Option<i64> Main() { return Last<i64>(7_i64); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Last".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("a generic enum constructor assigned to a typed mutable local must retain its enclosing specialization");

    beskid_codegen::validate_artifact(&artifact).expect("generic enum assignment artifact");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Last#generic_")));
}

#[test]
fn parsed_program_emits_only_call_derived_generic_specializations_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "i32 Keep<T>(T value) { return 7; } i32 Unused<T>(T value) { return 0; } i32 Main() { return Keep(1); }",
    );
    let items = find_function_definitions(input.database(), root);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Keep".into() },
            SyntaxModuleItem { key: items[1], symbol: "Unused".into() },
            SyntaxModuleItem { key: items[2], symbol: "Main".into() },
        ],
    )
    .expect("only the generic declaration proven by an actual direct call is materialized");

    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Keep#generic_")),
        "the actual generic call must materialize its exact call-derived ABI specialization"
    );
    assert!(
        artifact.functions.iter().all(|function| !function.name.starts_with("Unused#generic_")),
        "a generic declaration without an actual direct call must not be materialized"
    );
}

#[test]
fn parsed_program_skips_uncalled_generic_template_bodies_without_an_environment() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Inner<T>(T value) { return; } unit Outer<T>(T value) { Inner<T>(value); return; } unit Main() { return; }",
    );
    let items = find_function_definitions(input.database(), root);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Inner".into() },
            SyntaxModuleItem { key: items[1], symbol: "Outer".into() },
            SyntaxModuleItem { key: items[2], symbol: "Main".into() },
        ],
    )
    .expect("uncalled generic templates are not executable roots");

    assert_eq!(artifact.functions.len(), 1);
    assert_eq!(artifact.functions[0].name, "Main");
}

#[test]
fn parsed_program_rejects_a_generic_direct_call_without_a_provable_specialization() {
    let (input, isa, root) = item_fixture_with_root("unit Missing<T>() { return; } unit Main() { Missing(); }");
    let items = find_function_definitions(input.database(), root);

    let error = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Missing".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect_err("a generic direct call without source-proven arguments must fail closed");

    // Nothing fixes `T` in `Missing()`: the legality gate reports E1203 at the call before ISLE.
    let rendered = error.to_string();
    assert!(rendered.contains("E1203"), "{rendered}");
    assert!(rendered.contains("CallExpression@"), "{rendered}");
    assert!(!rendered.contains("MissingRuleOrFact"), "{rendered}");
}

#[test]
fn parsed_program_specializes_generic_string_not_equal_as_content_comparison() {
    let (input, isa, root) = item_fixture_with_root(
        "unit NotEqual<T>(T actual, T expected) { if actual != expected { return; } return; } unit Main() { NotEqual(\"left\", \"right\"); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "NotEqual".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("generic string != lowers through its exact specialization");

    let not_equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("NotEqual#generic_"))
        .expect("specialized NotEqual<string> function");
    let clif = not_equal.function.display().to_string();
    assert_string_content_comparison(&clif, StringComparison::NotEqual, "NotEqual<string>");
}

#[test]
fn parsed_program_keeps_generic_nominal_pointer_equal_as_identity_comparison() {
    let (input, isa, root) = item_fixture_with_root(
        "type Box<T> { i64 value } unit Equal<T>(T actual, T expected) { if actual == expected { return; } return; } unit Main() { Box<i64> value = Box<i64> { value: 0_i64 }; Equal(value, value); }",
    );
    let db = input.database();
    let items = find_function_definitions(db, root);
    let equal = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Equal"))
        .expect("generic Equal function");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main function");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: equal, symbol: "Equal".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("generic nominal pointer equality lowers through its exact specialization");

    let equal = artifact
        .functions
        .iter()
        .find(|function| function.name.starts_with("Equal#generic_"))
        .expect("specialized Equal<Box<i64>> function");
    let clif = equal.function.display().to_string();
    assert!(!clif.contains("%str_eq"), "nominal POINTER specialization must not call str_eq: {clif}");
    assert!(clif.contains("icmp eq v0, v1"), "nominal POINTER specialization must retain identity equality: {clif}");
}

#[test]
fn parsed_program_specializes_zero_argument_generic_factory_without_hir() {
    // Channel<T> Create<T>() collapses to POINTER at the ABI layer. Item ABI must still refuse a
    // fixed signature so module emission registers SpecializedItem, matching call-site imports.
    let (input, isa, root) = item_fixture_with_root(
        "type Channel<T> { i64 handle } Channel<T> Create<T>() { return Channel<T> { handle: 0_i64 }; } unit Main() { Channel<i64> ch = Create<i64>(); return; }",
    );
    let items = find_function_definitions(input.database(), root);
    let create = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Create"))
        .expect("Create");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    assert_eq!(beskid_queries::item_abi_signature(input.database(), create).expect("generic item ABI"), None);

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: create, symbol: "Create".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("zero-argument generic factories specialize through call-derived ABI identity");

    beskid_codegen::validate_artifact(&artifact)
        .expect("specialized factory imports must resolve against module declarations");
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Create#generic_")),
        "generic factory must emit a mangled specialization, not a bare Item identity"
    );
}

#[test]
fn parsed_program_specializes_generic_aggregate_literal_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Pair<T> { T value, i64 rest } Pair<T> Make<T>(T value) { return Pair<T> { value: value, rest: 9_i64 }; } i64 Main() { Pair<i32> pair = Make<i32>(7); return pair.rest; }",
    );
    let items = find_function_definitions(input.database(), root);
    let make = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Make"))
        .expect("Make");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: make, symbol: "Make".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("generic aggregate literals must lower through the enclosing item specialization");

    beskid_codegen::validate_artifact(&artifact)
        .expect("the specialized aggregate factory must resolve its data and call imports");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Make#generic_")));
}

#[test]
fn parsed_program_specializes_generic_pattern_payload_field_layout_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "type Applied<T> { T value, i64 rest } enum Result<T> { Ok(Applied<T> success), Err(i64 error) } i64 Rest<T>(Result<T> result) { return match result { Result::Ok(success) => success.rest, Result::Err(_) => -1_i64, }; } i64 Main(Result<i32> result) { return Rest<i32>(result); }",
    );
    let items = find_function_definitions(input.database(), root);
    let rest = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Rest"))
        .expect("Rest");
    let main = items
        .iter()
        .copied()
        .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: rest, symbol: "Rest".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("generic pattern payload projections must use the enclosing item specialization");

    beskid_codegen::validate_artifact(&artifact)
        .expect("the specialized generic match helper must resolve its call imports");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Rest#generic_")));
}

#[test]
fn generic_local_annotated_with_a_type_parameter_lowers_for_scalar_and_string() {
    let (input, isa, root) = item_fixture_with_root(
        "T Same<T>(T value) { T held = value; return held; } i64 Main() { string text = Same(\"s\"); return Same(7_i64); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "Same".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("a local whose annotation is a type parameter takes its type from the specialization");

    beskid_codegen::validate_artifact(&artifact).expect("generic local artifact");
    let specializations = artifact.functions.iter().filter(|function| function.name.starts_with("Same#generic_")).count();
    assert_eq!(specializations, 2, "one specialization each for i64 and string");
}

#[test]
fn parsed_program_packs_generic_bulk_arguments_with_the_call_specialization() {
    let (input, isa, root) = item_fixture_with_root(
        "T First<T>(bulk T[] values) { return values[0]; } i64 Main() { return First<i64>(4, 5, 6) + First(7_i64); }",
    );
    let items = find_function_definitions(input.database(), root);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: items[0], symbol: "First".into() },
            SyntaxModuleItem { key: items[1], symbol: "Main".into() },
        ],
    )
    .expect("a generic bulk call packs its arguments as the specialized element type");

    beskid_codegen::validate_artifact(&artifact).expect("generic bulk artifact");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("First#generic_")));
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main lowers");
    let clif = main.function.display().to_string();
    assert_eq!(clif.matches("store").count(), 4, "four i64 elements are stored: {clif}");
}

#[test]
fn generic_method_on_a_nested_enum_pattern_binding_is_specialized() {
    // `failures.Count()` where `failures` is the `List<i64>` payload bound by the nested pattern
    // `Outcome::Error(Problem::Failed(failures))` (Core.Parallel ParallelSuite shape): the receiver
    // has no written type, so its applied owner comes from the binding's proven source identity.
    let (input, isa, root) = item_fixture_with_root(
        "type List<T> { T[] values, i64 count, i64 Count() { return count; } } enum Problem { Failed(List<i64> failures), Empty } enum Outcome<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main(Outcome<i64, Problem> result) { match result { Outcome::Error(Problem::Failed(failures)) => { return failures.Count(); }, _ => { return 0_i64; }, }; return 0_i64; }",
    );
    let method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("List.Count method");
    let main = find_function_definitions(input.database(), root)[0];
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "List_Count".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a generic method on a nested enum-pattern binding must have a call-derived specialization");

    beskid_codegen::validate_artifact(&artifact).expect("nested pattern receiver artifact");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("List_Count#generic_")), "{names:?}");
}

#[test]
fn method_with_its_own_type_parameters_on_a_plain_owner_is_only_emitted_specialized() {
    // `Scope.SpawnTry<T>` shape: a method's own type parameters make it generic, so its body
    // (here a nested explicit `Id<T>(..)` over a function-typed parameter) is walked only in the
    // environment of a concrete call, never as a declaration-level item.
    let (input, isa, root) = item_fixture_with_root(
        "T Id<T>(T value) { return value; } type Runner { i64 id, T Apply<T>(() => T body) { return Id<T>(body()); } } i64 Main(Runner runner) { return runner.Apply<i64>(() => 7_i64); }",
    );
    let method = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MethodDefinition)
        .expect("Runner.Apply method");
    let main = find_function_definitions(input.database(), root)
        .into_iter()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main item");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: method, symbol: "Runner_Apply".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a method-generic body is lowered only through its call-derived specialization");

    beskid_codegen::validate_artifact(&artifact).expect("method-generic artifact");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("Runner_Apply#generic_")), "{names:?}");
    assert!(!names.contains(&"Runner_Apply"), "no declaration-level instance of a generic method: {names:?}");
}

#[test]
fn nested_pattern_bindings_of_a_call_match_in_a_generic_body_are_specialized() {
    // Core.Parallel `Reorder<T>` shape: `match input.Receive()` inside a generic body binds
    // `Result::Ok(Sequenced::Item(sequence, value))`, and both bindings feed a generic-owner method
    // call. The callee's instance in the enclosing environment applies the scrutinee's result.
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } enum Item<T> { Entry(i64 sequence, T value), Done } type Bag<V> { V[] values, i64 count, Bag<V> Put(i64 key, V value) { return Bag<V> { values: values, count: count + key }; } } type Source<T> { T value, Result<Item<T>, string> Next() { return Result::Ok(Item::Entry(1_i64, value)); } } i64 Drain<T>(Source<T> source, Bag<T> bag) { match source.Next() { Result::Ok(Item::Entry(sequence, value)) => { Bag<T> next = bag.Put(sequence, value); return next.count; }, _ => { return 0_i64; }, }; return 0_i64; } i64 Main(Source<i64> source, Bag<i64> bag) { return Drain<i64>(source, bag); }",
    );
    let items = find_function_definitions(input.database(), root);
    let named = |name: &str| {
        items
            .iter()
            .copied()
            .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some(name))
            .expect("named item")
    };
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: named("Drain"), symbol: "Drain".into() },
            SyntaxModuleItem { key: named("Main"), symbol: "Main".into() },
        ],
    )
    .expect("call-match bindings in a generic body take the callee instance's applied result");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("Drain#generic_")), "{names:?}");
}

#[test]
fn contextual_generic_enum_constructor_argument_of_a_receiver_bound_parameter_is_specialized() {
    // Core.Parallel `output.Send(Sequenced::Item(next, item))` shape: the receiver binds the
    // parameter's generic, so the constructor is checked against that type instead of inferring.
    let (input, isa, root) = item_fixture_with_root(
        "type Port<T> { i64 id, unit Send(T value) { return; } } enum Item<T> { Entry(i64 sequence, T value), Done } unit Forward<T>(Port<Item<T>> output, T value) { output.Send(Item::Entry(1_i64, value)); return; } unit Main(Port<Item<i64>> output) { Forward<i64>(output, 7_i64); output.Send(Item::Entry(2_i64, 8_i64)); return; }",
    );
    let items = find_function_definitions(input.database(), root);
    let named = |name: &str| {
        items
            .iter()
            .copied()
            .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some(name))
            .expect("named item")
    };
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: named("Forward"), symbol: "Forward".into() },
            SyntaxModuleItem { key: named("Main"), symbol: "Main".into() },
        ],
    )
    .expect("a constructor argument of an already-bound generic parameter specializes the call");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("Forward#generic_")), "{names:?}");
}

#[test]
fn typed_lambda_parameter_receiver_in_a_generic_body_is_specialized() {
    // Core.Parallel `Collect` shape: `(List<T> leaf, T value) => leaf.Push(value)` inside a generic
    // body; the lambda parameter's written type is applied through the enclosing environment.
    let (input, isa, root) = item_fixture_with_root(
        "type Bag<V> { V[] values, i64 count, Bag<V> Put(V value) { return Bag<V> { values: values, count: count + 1_i64 }; } } L Fold<T, L>(L seed, T value, (L, T) => L step) { return step(seed, value); } Bag<T> Gather<T>(Bag<T> seed, T value) { return Fold<T, Bag<T>>(seed, value, (Bag<T> bag, T item) => bag.Put(item)); } i64 Main(Bag<i64> bag) { Bag<i64> next = Gather<i64>(bag, 7_i64); return next.count; }",
    );
    let items = find_function_definitions(input.database(), root);
    let named = |name: &str| {
        items
            .iter()
            .copied()
            .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some(name))
            .expect("named item")
    };
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: named("Gather"), symbol: "Gather".into() },
            SyntaxModuleItem { key: named("Main"), symbol: "Main".into() },
        ],
    )
    .expect("a typed lambda parameter receiver in a generic body specializes the method call");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("Gather#generic_") && !name.contains("#lambda_")), "{names:?}");
    // The lambda is emitted once for the `Gather<i64>` instance, in that environment.
    assert!(names.iter().any(|name| name.starts_with("Gather#generic_") && name.contains("#lambda_")), "{names:?}");
}

#[test]
fn capturing_lambda_in_a_generic_body_has_an_instance_closure_layout() {
    // `Drive<T, L, R>` shape: a lambda captures a value of the enclosing generic type, so its
    // closure layout (scalar `i64` here, a managed reference for `Bag<i64>`) depends on the instance.
    let (input, isa, root) = item_fixture_with_root(
        "type Bag<V> { V[] values, i64 count, Bag<V> Put(V value) { return Bag<V> { values: values, count: count + 1_i64 }; } } L Fold<T, L>(L seed, T value, (L, T) => L step) { return step(seed, value); } Bag<T> Keep<T>(Bag<T> seed, T value, T extra) { return Fold<T, Bag<T>>(seed, value, (Bag<T> bag, T item) => bag.Put(extra)); } i64 Main(Bag<i64> bag, Bag<Bag<i64>> nested) { Bag<i64> next = Keep<i64>(bag, 7_i64, 8_i64); Bag<Bag<i64>> outer = Keep<Bag<i64>>(nested, bag, next); return next.count + outer.count; }",
    );
    let items = find_function_definitions(input.database(), root);
    let named = |name: &str| {
        items
            .iter()
            .copied()
            .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some(name))
            .expect("named item")
    };
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: named("Keep"), symbol: "Keep".into() },
            SyntaxModuleItem { key: named("Main"), symbol: "Main".into() },
        ],
    )
    .expect("a capturing lambda in a generic body is emitted per instance");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    let entries = names.iter().filter(|name| name.starts_with("Keep#generic_") && name.contains("#lambda_")).count();
    assert_eq!(entries, 2, "one closure entry per instance: {names:?}");
}

#[test]
fn genericless_constructor_lambda_body_takes_the_expected_result() {
    // ParallelSuite `s.SpawnTry<i64>(() => Result::Error("no input"))`: the lambda body is a
    // constructor whose type only the expected function result (`Outcome<i64, string>`) proves.
    let (input, isa, root) = item_fixture_with_root(
        "enum Outcome<T, E> { Ok(T value), Error(E error) } type Runner { i64 id, i64 Try<T>(() => Outcome<T, string> body) { return 1_i64; } } i64 Main(Runner runner) { return runner.Try<i64>(() => Outcome::Error(\"no input\")); }",
    );
    let items = find_function_definitions(input.database(), root);
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a constructor lambda body is typed by the expected function result");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("__beskid_lambda_entry_syntax_")), "{names:?}");
}

#[test]
fn contract_call_on_a_captured_bounded_parameter_in_a_lambda_uses_the_instance_witness() {
    // `ParallelSeq.Reduce<C>` shape: `(T acc, T value) => combiner.Combine(acc, value)` captures a
    // contract-bounded parameter; the lambda body dispatches through the enclosing instance's witness.
    let (input, isa, root) = item_fixture_with_root(
        "contract Joins<T> { T Combine(T left, T right); } type Plus : Joins<i64> { i64 id, pub i64 Combine(i64 left, i64 right) { return left + right; } } L Fold<T, L>(L seed, T value, (L, T) => L step) { return step(seed, value); } T Merge<T, C>(T seed, T value, C combiner) where C: Joins<T> { return Fold<T, T>(seed, value, (T acc, T item) => combiner.Combine(acc, item)); } i64 Main(Plus plus) { return Merge<i64, Plus>(1_i64, 2_i64, plus); }",
    );
    let items = find_function_definitions(input.database(), root);
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a contract call on a captured bounded parameter lowers in the instance");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(
        names.iter().any(|name| name.starts_with("Merge#") && name.contains("#generic_") && name.contains("#lambda_")),
        "{names:?}"
    );
}

#[test]
fn constructor_returned_by_a_lambda_in_a_generic_body_takes_the_callee_result_type() {
    // `ParallelSeq.Split` shape: `Scopes.Run<R>((Scope s) => { return Result::Ok(..); })` inside a
    // generic body; the constructor's type is the callee parameter's function result with the
    // explicit type argument applied, interpreted in the enclosing instance.
    let (input, isa, root) = item_fixture_with_root(
        "enum Outcome<T, E> { Ok(T value), Error(E error) } i64 Run<T>(() => Outcome<T, string> body) { return 1_i64; } i64 Wrap<R>(R value) { return Run<R>(() => { return Outcome::Ok(value); }); } i64 Main() { return Wrap<i64>(5_i64); }",
    );
    let items = find_function_definitions(input.database(), root);
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a constructor returned by a lambda in a generic body lowers in the instance");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(
        names.iter().any(|name| name.starts_with("Wrap#") && name.contains("#generic_") && name.contains("#lambda_")),
        "{names:?}"
    );
}

#[test]
fn match_on_a_call_through_a_typed_local_of_a_lambda_in_a_generic_body_is_specialized() {
    // `ParallelSeq.Split` shape: `Task<R> upper = s.Spawn<R>(..); return match upper.Join() { .. }`
    // inside a lambda of a generic body; the scrutinee's applied enum is read through the typed
    // local's identity in the enclosing instance.
    let (input, isa, root) = item_fixture_with_root(
        "enum Outcome<T, E> { Ok(T value), Error(E error) } type Box<T> { T item, Outcome<T, string> Take() { return Outcome::Ok(this.item); } } Box<T> Make<T>(T item) { return Box<T> { item: item }; } i64 Run<T>(() => Outcome<T, string> body) { return 1_i64; } i64 Wrap<R>(R value) { return Run<R>(() => { Box<R> b = Make<R>(value); return match b.Take() { Outcome::Ok(v) => Outcome::Ok(v), Outcome::Error(e) => Outcome::Error(e) }; }); } i64 Main() { return Wrap<i64>(5_i64); }",
    );
    let items = find_function_definitions(input.database(), root);
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a match on a call through a typed local of a lambda in a generic body lowers in the instance");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("Wrap#") && name.contains("#lambda_")), "{names:?}");
    assert!(names.iter().any(|name| name.starts_with("Take#") && name.contains("#generic_")), "{names:?}");
}

#[test]
fn receiver_bound_constructor_argument_inside_a_lambda_of_a_generic_body_is_specialized() {
    // Core.Parallel shape inside a lambda: `(T v) => output.Send(Item::Entry(1_i64, v))` with
    // `Port<Item<T>> output` captured by a lambda of `Forward<T>`; the receiver's identity in the
    // instance binds the parameter's generic, so the constructor is checked against it.
    let (input, isa, root) = item_fixture_with_root(
        "type Port<T> { i64 id, unit Send(T value) { return; } } enum Item<T> { Entry(i64 sequence, T value), Done } unit Each<T>(T value, (T) => unit f) { f(value); return; } unit Forward<T>(Port<Item<T>> output, T value) { Each<T>(value, (T v) => output.Send(Item::Entry(1_i64, v))); return; } unit Main(Port<Item<i64>> output) { Forward<i64>(output, 7_i64); return; }",
    );
    let items = find_function_definitions(input.database(), root);
    let named = |name: &str| {
        items
            .iter()
            .copied()
            .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some(name))
            .expect("named item")
    };
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: named("Forward"), symbol: "Forward".into() },
            SyntaxModuleItem { key: named("Main"), symbol: "Main".into() },
        ],
    )
    .expect("a receiver-bound constructor argument inside a lambda of a generic body specializes the call");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("Forward#") && name.contains("#lambda_")), "{names:?}");
}

#[test]
fn chained_generic_call_receiver_in_a_lambda_of_a_generic_body_is_specialized() {
    // `ParallelSeq.Collect` shape: `(List<T> leaf) => List.New<List<T>>().Push(leaf)` where
    // `Push` is a method of a generic owner and the receiver is itself a
    // generic call with explicit arguments naming the enclosing parameter.
    let (input, isa, root) = item_fixture_with_root(
        "type Bag<V> { i64 count, Bag<V> Put(V value) { return Bag<V> { count: this.count + 1_i64 }; } } Bag<V> NewBag<V>() { return Bag<V> { count: 0_i64 }; } R Fold<T, L, R>(T value, () => L start, (L, T) => L step, (L) => R finish) { return finish(step(start(), value)); } Bag<Bag<T>> Collect<T>(T value) { return Fold<T, Bag<T>, Bag<Bag<T>>>(value, () => NewBag<T>(), (Bag<T> leaf, T v) => leaf.Put(v), (Bag<T> leaf) => NewBag<Bag<T>>().Put(leaf)); } i64 Main() { return Collect<i64>(5_i64).count; }",
    );
    let items = find_function_definitions(input.database(), root);
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a chained generic call receiver in a lambda of a generic body lowers in the instance");
    let names = artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>();
    assert!(names.iter().filter(|name| name.starts_with("Collect#") && name.contains("#lambda_")).count() == 3, "{names:?}");
}
