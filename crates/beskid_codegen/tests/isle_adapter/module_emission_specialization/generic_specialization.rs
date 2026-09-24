//! Call-derived generic specializations, rejections, and specialized layouts.

use super::super::support::{
    SyntaxModuleItem, find_function_definitions, item_fixture_with_root, item_name, lower_syntax_program,
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
