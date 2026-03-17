use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::hir::{AstProgram, HirProgram, lower_program};
use beskid_analysis::resolve::{ResolveError, Resolver};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{CallLoweringKind, TypeInfo};
use beskid_analysis::types::{TypeError, type_program};

use crate::syntax::util::parse_program_ast;

fn resolve_and_type(source: &str) -> Result<beskid_analysis::types::TypeResult, Vec<TypeError>> {
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let hir: Spanned<HirProgram> = lower_program(&ast);
    let resolution =
        Resolver::new()
            .resolve_program(&hir)
            .unwrap_or_else(|errors: Vec<ResolveError>| {
                panic!("expected resolver to succeed, got errors: {errors:?}")
            });
    type_program(&hir, &resolution)
}

#[test]
fn typing_records_method_dispatch_call_kind() {
    let result = resolve_and_type(
        "type Counter { i64 value } impl Counter { i64 Get() { return this.value; } } i64 main() { Counter c = Counter { value: 42 }; return c.Get(); }",
    )
    .expect("expected typing to succeed");

    assert!(
        result
            .call_kinds
            .values()
            .any(|kind| matches!(kind, CallLoweringKind::MethodDispatch { .. })),
        "expected at least one MethodDispatch call kind, got: {:?}",
        result.call_kinds
    );
}

#[test]
fn typing_allows_declared_conformance_argument_coercion() {
    let result = resolve_and_type(
        "contract Service { i64 run(i64 x); } type Worker : Service { i64 base } impl Worker { i64 run(i64 x) { return this.base + x; } } i64 apply(Service s) { return s.run(1); } i64 main() { Worker w = Worker { base: 41 }; return apply(w); }",
    );
    if let Err(errors) = &result {
        panic!("expected conformance-based argument coercion typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected contract coercion typing failure");
}

#[test]
fn typing_records_contract_dispatch_call_kind() {
    let result = resolve_and_type("contract Service { i64 run(i64 x); } i64 apply(Service s) { return s.run(1); }")
        .expect("expected typing to succeed");

    assert!(
        result
            .call_kinds
            .values()
            .any(|kind| matches!(kind, CallLoweringKind::ContractDispatch { .. })),
        "expected at least one ContractDispatch call kind, got: {:?}",
        result.call_kinds
    );
}

#[test]
fn typing_records_item_call_kind() {
    let result = resolve_and_type("i64 add(i64 a, i64 b) { return a + b; } i64 main() { return add(1, 2); }")
        .expect("expected typing to succeed");

    assert!(
        result
            .call_kinds
            .values()
            .any(|kind| matches!(kind, CallLoweringKind::ItemCall { .. })),
        "expected at least one ItemCall call kind, got: {:?}",
        result.call_kinds
    );
}

#[test]
fn typing_records_callable_value_call_kind() {
    let result = resolve_and_type("i64 main() { let add = (i64 x, i64 y) => x + y; return add(20, 22); }")
        .expect("expected typing to succeed");

    assert!(
        result
            .call_kinds
            .values()
            .any(|kind| matches!(kind, CallLoweringKind::CallableValueCall)),
        "expected at least one CallableValueCall kind, got: {:?}",
        result.call_kinds
    );
}

#[test]
fn typing_method_call_on_struct_succeeds() {
    let result = resolve_and_type(
        "type Counter { i64 value } impl Counter { i64 Get() { return this.value; } } i64 main() { Counter c = Counter { value: 42 }; return c.Get(); }",
    );
    if let Err(errors) = &result {
        panic!("expected method call typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected method call typing failure");
}

#[test]
fn typing_method_dispatch_is_receiver_aware() {
    let result = resolve_and_type(
        "type A { i64 value } type B { i64 value } impl A { i64 Get() { return this.value; } } impl B { i64 Get() { i64 delta = 1; return this.value + delta; } } i64 main() { A a = A { value: 20 }; B b = B { value: 21 }; return a.Get() + b.Get(); }",
    );
    if let Err(errors) = &result {
        panic!("expected receiver-aware method dispatch typing to succeed, got errors: {errors:?}");
    }
    assert!(
        result.is_ok(),
        "unexpected receiver-aware dispatch typing failure"
    );
}

#[test]
fn typing_rejects_identity_equality_on_numeric_values() {
    let result = resolve_and_type("bool main() { return 1 === 1; }");
    let errors = result.expect_err("expected numeric identity equality to be rejected");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidBinaryOp { .. })),
        "expected InvalidBinaryOp for numeric identity equality, got: {errors:?}"
    );
}

#[test]
fn typing_allows_identity_equality_on_named_values() {
    let result = resolve_and_type(
        "type User { i64 id } bool main() { User a = User { id: 1 }; User b = a; return a === b; }",
    );
    if let Err(errors) = &result {
        panic!("expected named identity equality typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected named identity equality typing failure");
}

#[test]
fn typing_rejects_compound_assign_on_non_numeric_non_string() {
    let result = resolve_and_type("unit main() { bool mut flag = true; flag += false; }");
    let errors = result.expect_err("expected invalid compound assignment on bool");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::UnsupportedExpression { .. })),
        "expected UnsupportedExpression for bool += bool, got: {errors:?}"
    );
}

#[test]
fn typing_allows_string_compound_add_assign() {
    let result = resolve_and_type("unit main() { string mut s = \"a\"; s += \"b\"; }");
    if let Err(errors) = &result {
        panic!("expected string += typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected string += typing failure");
}

#[test]
fn typing_allows_event_member_subscribe_and_unsubscribe() {
    let result = resolve_and_type(
        "type User { event{4} Created(string payload) } unit main() { User mut u = User { }; unit(string) handler = (string payload) => { return; }; u.Created += handler; u.Created -= handler; }",
    );
    if let Err(errors) = &result {
        panic!("expected event +=/-= typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected event +=/-= typing failure");
}

#[test]
fn typing_rejects_add_assign_handler_on_non_event_target() {
    let result = resolve_and_type(
        "type User { i64 count } unit main() { User mut u = User { count: 0 }; unit(string) handler = (string payload) => { return; }; u.count += handler; }",
    );
    let errors = result.expect_err("expected non-event += handler rejection");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidEventSubscriptionTarget { .. })),
        "expected InvalidEventSubscriptionTarget for non-event += handler, got: {errors:?}"
    );
}

#[test]
fn typing_rejects_sub_assign_handler_on_non_event_target() {
    let result = resolve_and_type(
        "type User { i64 count } unit main() { User mut u = User { count: 0 }; unit(string) handler = (string payload) => { return; }; u.count -= handler; }",
    );
    let errors = result.expect_err("expected non-event -= handler rejection");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidEventSubscriptionTarget { .. })),
        "expected InvalidEventSubscriptionTarget for non-event -= handler, got: {errors:?}"
    );
}

#[test]
fn typing_rejects_zero_event_capacity() {
    let result = resolve_and_type("type User { event{0} Created(string payload) } unit main() { return; }");
    let errors = result.expect_err("expected zero event capacity rejection");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidEventCapacity { .. })),
        "expected InvalidEventCapacity for event{{0}}, got: {errors:?}"
    );
}

#[test]
fn typing_allows_owner_event_invoke() {
    let result = resolve_and_type(
        "type User { event{4} Created(string payload) } impl User { unit Emit(string payload) { this.Created(payload); } } unit main() { User mut u = User { }; u.Emit(\"ok\"); }",
    );
    if let Err(errors) = &result {
        panic!("expected owner event invoke typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected owner event invoke typing failure");
}

#[test]
fn typing_rejects_non_owner_event_invoke() {
    let result = resolve_and_type(
        "type User { event{4} Created(string payload) } unit main() { User mut u = User { }; u.Created(\"x\"); }",
    );
    let errors = result.expect_err("expected non-owner event invoke rejection");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidEventInvocationScope { .. })),
        "expected InvalidEventInvocationScope for non-owner event invoke, got: {errors:?}"
    );
}

#[test]
fn typing_reports_unknown_method_call_target() {
    let result = resolve_and_type(
        "type Counter { i64 value } i64 main() { Counter c = Counter { value: 1 }; return c.Missing(); }",
    );
    let errors = result.expect_err("expected unknown method call target");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::UnknownCallTarget { .. })),
        "expected UnknownCallTarget error, got: {errors:?}"
    );
}

#[test]
fn typing_literals_succeeds() {
    let result = resolve_and_type("unit main() { i64 x = 1; bool y = true; }");
    assert!(result.is_ok());
}

#[test]
fn typing_reports_mismatch() {
    let result = resolve_and_type("unit main() { bool x = 1; }");
    let errors = result.expect_err("expected type mismatch error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::TypeMismatch { .. }))
    );
}

#[test]
fn typing_reports_non_bool_condition() {
    let result = resolve_and_type("unit main() { if 1 { i64 x = 1; } }");
    let errors = result.expect_err("expected non-bool condition error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::NonBoolCondition { .. }))
    );
}

#[test]
fn typing_reports_return_mismatch() {
    let result = resolve_and_type("i64 main() { return true; }");
    let errors = result.expect_err("expected return type mismatch");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::TypeMismatch { .. }))
    );
}

#[test]
fn typing_function_calls_succeeds() {
    let result = resolve_and_type(
        "i64 add(i64 a, i64 b) { return a + b; } unit main() { i64 x = add(1, 2); }",
    );
    assert!(result.is_ok());
}

#[test]
fn typing_generic_function_call_succeeds() {
    let result =
        resolve_and_type("T id<T>(T x) { return x; } unit main() { i64 x = id<i64>(1); }");
    if let Err(errors) = &result {
        panic!("expected generic call typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok());
}

#[test]
fn typing_reports_missing_generic_args_for_call() {
    let result = resolve_and_type("T id<T>(T x) { return x; } unit main() { i64 x = id(1); }");
    let errors = result.expect_err("expected missing generic args error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::MissingTypeArguments { .. }))
    );
}

#[test]
fn typing_reports_generic_arg_mismatch_for_call() {
    let result =
        resolve_and_type("T id<T>(T x) { return x; } unit main() { i64 x = id<i64, string>(1); }");
    let errors = result.expect_err("expected generic arg mismatch error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::GenericArgumentMismatch { .. }))
    );
}

#[test]
fn typing_reports_missing_generic_args_for_type() {
    let result =
        resolve_and_type("type Box<T> { T value } unit main() { Box x = Box { value: 1 }; }");
    let errors = result.expect_err("expected missing generic args for type");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::MissingTypeArguments { .. }))
    );
}

#[test]
fn typing_reports_generic_arg_mismatch_for_type() {
    let result = resolve_and_type(
        "type Box<T> { T value } unit main() { Box<i64, string> x = Box<i64> { value: 1 }; }",
    );
    let errors = result.expect_err("expected generic arg mismatch for type");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::GenericArgumentMismatch { .. }))
    );
}

#[test]
fn typing_reports_call_arity_mismatch() {
    let result = resolve_and_type(
        "i64 add(i64 a, i64 b) { return a + b; } unit main() { i64 x = add(1); }",
    );
    let errors = result.expect_err("expected call arity mismatch");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::CallArityMismatch { .. }))
    );
}

#[test]
fn typing_struct_literal_and_member_access() {
    let result = resolve_and_type(
        "type User { i64 id, string name } unit main() { User u = User { id: 1, name: \"a\" }; i64 x = u.id; }",
    );
    if let Err(errors) = &result {
        panic!("expected struct literal/member typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected typing failure");
}

#[test]
fn typing_reports_missing_struct_field() {
    let result = resolve_and_type(
        "type User { i64 id, string name } unit main() { User u = User { id: 1 }; }",
    );
    let errors = result.expect_err("expected missing struct field");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::MissingStructField { .. })),
        "expected MissingStructField error, got: {errors:?}"
    );
}

#[test]
fn typing_match_expression_unifies_types() {
    let result = resolve_and_type(
        "enum Choice { Some(string value), None } unit main() { Choice opt = Choice::None(); string x = match opt { Choice::Some(value) => value, Choice::None => \"none\", }; }",
    );
    if let Err(errors) = &result {
        panic!("expected match typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected match typing failure");
}

#[test]
fn typing_string_interpolation_with_variable_succeeds() {
    let result =
        resolve_and_type("unit main() { string name = \"Ada\"; string msg = \"hi ${name}\"; }");
    if let Err(errors) = &result {
        panic!("expected interpolated string typing to succeed, got errors: {errors:?}");
    }
    assert!(
        result.is_ok(),
        "unexpected interpolated string typing failure"
    );
}

#[test]
fn typing_string_interpolation_with_full_expression_succeeds() {
    let result = resolve_and_type(
        "unit main() { string name = \"Ada\"; string suffix = \"!\"; string msg = \"hi ${name + suffix}\"; }",
    );
    if let Err(errors) = &result {
        panic!("expected interpolated expression typing to succeed, got errors: {errors:?}");
    }
    assert!(
        result.is_ok(),
        "unexpected interpolated expression typing failure"
    );
}

#[test]
fn typing_records_cast_intent_for_numeric_mismatch() {
    let result = resolve_and_type("unit main() { i32 x = 1; i64 y = x; }")
        .expect("expected typing to succeed with cast intent");
    assert_eq!(
        result.cast_intents.len(),
        1,
        "expected exactly one cast intent"
    );

    let intent = &result.cast_intents[0];
    let from = result.types.get(intent.from);
    let to = result.types.get(intent.to);
    assert_eq!(from, Some(&TypeInfo::Primitive(HirPrimitiveType::I32)));
    assert_eq!(to, Some(&TypeInfo::Primitive(HirPrimitiveType::I64)));
}

#[test]
fn typing_cast_intents_are_sorted_by_source_span() {
    let result = resolve_and_type("unit main() { i32 a = 1; i64 b = a; i32 c = 2; i64 d = c; }")
        .expect("expected typing to succeed with cast intents");

    assert!(
        result.cast_intents.len() >= 2,
        "expected at least two cast intents"
    );
    for pair in result.cast_intents.windows(2) {
        assert!(
            pair[0].span.start <= pair[1].span.start,
            "cast intents are not sorted by span start: {:?}",
            result.cast_intents
        );
    }
}

#[test]
fn typing_cast_intents_preserve_source_line_spans() {
    let result = resolve_and_type(
        "unit main() {\n  i32 x = 1;\n  i64 y = x;\n  i32 z = 2;\n  i64 w = z;\n}",
    )
    .expect("expected typing to succeed with cast intents");

    let lines: Vec<usize> = result
        .cast_intents
        .iter()
        .map(|intent| intent.span.line_col_start.0)
        .collect();
    assert_eq!(
        lines,
        vec![3, 5],
        "unexpected cast-intent line mapping: {lines:?}"
    );
}

#[test]
fn typing_records_cast_intent_for_numeric_call_argument_mismatch() {
    let result = resolve_and_type(
        "i64 take(i64 v) { return v; } unit main() { i32 x = 1; i64 y = take(x); }",
    )
    .expect("expected typing to succeed with cast intent in call argument");

    assert!(
        !result.cast_intents.is_empty(),
        "expected cast intent for numeric call argument mismatch"
    );
}

#[test]
fn typing_records_cast_intent_for_numeric_return_mismatch() {
    let result = resolve_and_type("i64 main() { i32 x = 1; return x; }")
        .expect("expected typing to succeed with cast intent in return");

    assert!(
        !result.cast_intents.is_empty(),
        "expected cast intent for numeric return mismatch"
    );
}

#[test]
fn typing_cast_intent_accessor_finds_intent_by_span() {
    let result = resolve_and_type("unit main() { i32 x = 1; i64 y = x; }")
        .expect("expected typing to succeed with cast intent");
    let span = result.cast_intents[0].span;
    let found = result.cast_intent_for_span(span);
    assert!(
        found.is_some(),
        "expected cast intent to be retrievable by span"
    );
}

#[test]
fn typing_nested_match_expression_unifies_types() {
    let result = resolve_and_type(
        "enum Choice { Some(i32 value), None } unit main() { Choice x = Choice::Some(1); i32 y = match x { Choice::Some(v) => match x { Choice::Some(_) => v, Choice::None => 0, }, Choice::None => 0, }; }",
    );
    if let Err(errors) = &result {
        panic!("expected nested match typing to succeed, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected nested match typing failure");
}

#[test]
fn typing_reports_enum_pattern_arity_mismatch() {
    let result = resolve_and_type(
        "enum Choice { Some(i64 value), None } unit main() { Choice x = Choice::Some(1); i64 y = match x { Choice::Some() => 0, Choice::None => 1, }; }",
    );
    let errors = result.expect_err("expected enum pattern arity mismatch");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::EnumConstructorMismatch { .. })),
        "expected EnumConstructorMismatch error, got: {errors:?}"
    );
}

#[test]
fn typing_reports_enum_pattern_field_type_mismatch() {
    let result = resolve_and_type(
        "enum Choice { Some(i64 value), None } unit main() { Choice x = Choice::Some(1); i64 y = match x { Choice::Some(\"text\") => 0, Choice::None => 1, }; }",
    );
    let errors = result.expect_err("expected enum pattern field type mismatch");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::TypeMismatch { .. })),
        "expected TypeMismatch error, got: {errors:?}"
    );
}

#[test]
fn typing_grouped_expression_propagates_type() {
    let result = resolve_and_type("unit main() { i64 x = (1); }");
    assert!(
        result.is_ok(),
        "expected grouped expression typing to succeed"
    );
}

#[test]
fn typing_block_expression_propagates_unit_type() {
    let result = resolve_and_type("unit main() { unit x = { i64 y = 1; }; }");
    assert!(
        result.is_ok(),
        "expected block expression typing to succeed"
    );
}

#[test]
fn typing_reports_invalid_member_target_for_non_struct() {
    let result = resolve_and_type("unit main() { i64 x = 1; i64 y = x.foo; }");
    let errors = result.expect_err("expected invalid member target error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidMemberTarget { .. })),
        "expected InvalidMemberTarget error, got: {errors:?}"
    );
}

#[test]
fn typing_reports_enum_constructor_arity_mismatch() {
    let result = resolve_and_type(
        "enum Choice { Some(i64 value), None } unit main() { Choice x = Choice::Some(); }",
    );
    let errors = result.expect_err("expected enum constructor mismatch");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::EnumConstructorMismatch { .. })),
        "expected EnumConstructorMismatch error, got: {errors:?}"
    );
}

#[test]
fn typing_reports_unknown_struct_field() {
    let result = resolve_and_type(
        "type User { i64 id, string name } unit main() { User u = User { id: 1, name: \"a\" }; i64 x = u.age; }",
    );
    let errors = result.expect_err("expected unknown struct field");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::UnknownStructField { .. })),
        "expected UnknownStructField error, got: {errors:?}"
    );
}

#[test]
fn typing_for_loop_infers_iterator_type_from_iterable_contract() {
    let result = resolve_and_type(
        "
        enum Option { Some(i64 value), None }
        type Iter { i64 seed }
        impl Iter {
            Option Next() { return Option::Some(1); }
        }
        unit main() {
            Iter iter = Iter { seed: 0 };
            i64 mut sum = 0;
            for i in iter { sum += i; }
        }
        ",
    );
    assert!(result.is_ok(), "expected iterable for-loop typing to succeed");
}

#[test]
fn typing_for_loop_rejects_non_iterable_target() {
    let result = resolve_and_type("unit main() { i64 v = 1; for i in v { continue; } }");
    let errors = result.expect_err("expected non-iterable for target error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::NonIterableForTarget { .. })),
        "expected NonIterableForTarget error, got: {errors:?}"
    );
}

#[test]
fn typing_for_loop_rejects_next_returning_non_option() {
    let result = resolve_and_type(
        "
        type Iter { i64 seed }
        impl Iter {
            i64 Next() { return this.seed; }
        }
        unit main() {
            Iter iter = Iter { seed: 0 };
            for i in iter { continue; }
        }
        ",
    );
    let errors = result.expect_err("expected iterable Next non-option error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::IterableNextReturnNotOption { .. })),
        "expected IterableNextReturnNotOption error, got: {errors:?}"
    );
}

#[test]
fn typing_for_loop_rejects_next_with_non_zero_arity() {
    let result = resolve_and_type(
        "
        enum Option { Some(i64 value), None }
        type Iter { i64 seed }
        impl Iter {
            Option Next(i64 step) { return Option::None(); }
        }
        unit main() {
            Iter iter = Iter { seed: 0 };
            for i in iter { continue; }
        }
        ",
    );
    let errors = result.expect_err("expected iterable Next arity mismatch error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::IterableNextArityMismatch { .. })),
        "expected IterableNextArityMismatch error, got: {errors:?}"
    );
}

#[test]
fn typing_for_loop_rejects_option_some_payload_arity_mismatch() {
    let result = resolve_and_type(
        "
        enum Option { Some(i64 a, i64 b), None }
        type Iter { i64 seed }
        impl Iter {
            Option Next() { return Option::None(); }
        }
        unit main() {
            Iter iter = Iter { seed: 0 };
            for i in iter { continue; }
        }
        ",
    );
    let errors = result.expect_err("expected iterable Option::Some payload arity mismatch error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::IterableOptionSomeArityMismatch { .. })),
        "expected IterableOptionSomeArityMismatch error, got: {errors:?}"
    );
}

#[test]
fn typing_rejects_invalid_try_target() {
    let result = resolve_and_type("i64 main() { i64 x = 1; i64 y = x?; return y; }");
    let errors = result.expect_err("expected invalid try target typing error");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidTryTarget { .. })),
        "expected InvalidTryTarget error, got: {errors:?}"
    );
}

#[test]
fn typing_allows_try_on_result_and_unwraps_ok_payload_type() {
    let result = resolve_and_type(
        "enum Result { Ok(i64 value), Error(string message) } i64 main() { Result r = Result::Ok(42); i64 value = r?; return value; }",
    );
    if let Err(errors) = &result {
        panic!("expected try on Result to type-check, got errors: {errors:?}");
    }
    assert!(result.is_ok(), "unexpected try-on-result typing failure");
}
