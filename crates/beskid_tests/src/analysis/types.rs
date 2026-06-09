use beskid_analysis::hir::HirPrimitiveType;
use beskid_analysis::types::{CallLoweringKind, TypeError, TypeInfo};

use crate::support::pipeline::typecheck as resolve_and_type;

type ErrorMatcher = fn(&[TypeError]) -> bool;

struct TypeCase {
    name: &'static str,
    source: &'static str,
    expect_ok: bool,
    error_matcher: Option<ErrorMatcher>,
}

fn assert_type_case(case: &TypeCase) {
    let result = resolve_and_type(case.source);
    if case.expect_ok {
        if let Err(errors) = &result {
            panic!("{}: expected typing to succeed, got {errors:?}", case.name);
        }
    } else {
        let errors = result.expect_err(&format!("{}: expected typing error", case.name));
        let matcher = case
            .error_matcher
            .expect("error case must provide error_matcher");
        assert!(
            matcher(&errors),
            "{}: unexpected errors: {errors:?}",
            case.name
        );
    }
}

fn matches_non_iterable_for_target(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::NonIterableForTarget { .. }))
}

fn matches_iterable_next_not_option(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::IterableNextReturnNotOption { .. }))
}

fn matches_iterable_next_arity(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::IterableNextArityMismatch { .. }))
}

fn matches_iterable_option_some_arity(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::IterableOptionSomeArityMismatch { .. }))
}

fn matches_invalid_event_subscription(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidEventSubscriptionTarget { .. }))
}

fn matches_missing_type_args(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::MissingTypeArguments { .. }))
}

fn matches_generic_arg_mismatch(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::GenericArgumentMismatch { .. }))
}

fn matches_enum_constructor_mismatch(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::EnumConstructorMismatch { .. }))
}

fn matches_type_mismatch(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::TypeMismatch { .. }))
}

fn matches_invalid_binary_op(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidBinaryOp { .. }))
}

fn matches_unsupported_expression(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::UnsupportedExpression { .. }))
}

fn matches_invalid_event_capacity(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidEventCapacity { .. }))
}

fn matches_invalid_event_invoke_scope(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidEventInvocationScope { .. }))
}

fn matches_unknown_call_target(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::UnknownCallTarget { .. }))
}

fn matches_non_bool_condition(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::NonBoolCondition { .. }))
}

fn matches_call_arity_mismatch(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::CallArityMismatch { .. }))
}

fn matches_missing_struct_field(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::MissingStructField { .. }))
}

fn matches_invalid_member_target(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidMemberTarget { .. }))
}

fn matches_unknown_struct_field(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::UnknownStructField { .. }))
}

fn matches_invalid_try_target(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::InvalidTryTarget { .. }))
}

struct CallKindCase {
    name: &'static str,
    source: &'static str,
    predicate: fn(&CallLoweringKind) -> bool,
}

fn assert_call_kind_case(case: &CallKindCase) {
    let result =
        resolve_and_type(case.source).unwrap_or_else(|_| panic!("{}: expected typing to succeed", case.name));
    assert!(
        result.call_kinds.values().any(case.predicate),
        "{}: expected matching call kind, got {:?}",
        case.name,
        result.call_kinds
    );
}

const CALL_KIND_CASES: &[CallKindCase] = &[
    CallKindCase {
        name: "method_dispatch",
        source: "type Counter { i64 value } impl Counter { i64 Get() { return this.value; } } i64 Main() { Counter c = Counter { value: 42 }; return c.Get(); }",
        predicate: |kind| matches!(kind, CallLoweringKind::MethodDispatch { .. }),
    },
    CallKindCase {
        name: "contract_dispatch",
        source: "contract Service { i64 run(i64 x); } i64 apply(Service s) { return s.run(1); }",
        predicate: |kind| matches!(kind, CallLoweringKind::ContractDispatch { .. }),
    },
    CallKindCase {
        name: "item_call",
        source: "i64 add(i64 a, i64 b) { return a + b; } i64 Main() { return add(1, 2); }",
        predicate: |kind| matches!(kind, CallLoweringKind::ItemCall { .. }),
    },
    CallKindCase {
        name: "callable_value_call",
        source: "i64 Main() { let add = (i64 x, i64 y) => x + y; return add(20, 22); }",
        predicate: |kind| matches!(kind, CallLoweringKind::CallableValueCall),
    },
];

#[test]
fn typing_records_expected_call_kinds() {
    for case in CALL_KIND_CASES {
        assert_call_kind_case(case);
    }
}

const FOR_LOOP_ERROR_CASES: &[TypeCase] = &[
    TypeCase {
        name: "non_iterable_target",
        source: "unit Main() { i64 v = 1; for i in v { continue; } }",
        expect_ok: false,
        error_matcher: Some(matches_non_iterable_for_target),
    },
    TypeCase {
        name: "next_returning_non_option",
        source: "
            type Iter { i64 seed }
            impl Iter { i64 Next() { return this.seed; } }
            unit Main() { Iter iter = Iter { seed: 0 }; for i in iter { continue; } }
        ",
        expect_ok: false,
        error_matcher: Some(matches_iterable_next_not_option),
    },
    TypeCase {
        name: "next_with_non_zero_arity",
        source: "
            enum Option { Some(i64 value), None }
            type Iter { i64 seed }
            impl Iter { Option Next(i64 step) { return Option::None(); } }
            unit Main() { Iter iter = Iter { seed: 0 }; for i in iter { continue; } }
        ",
        expect_ok: false,
        error_matcher: Some(matches_iterable_next_arity),
    },
    TypeCase {
        name: "option_some_payload_arity_mismatch",
        source: "
            enum Option { Some(i64 a, i64 b), None }
            type Iter { i64 seed }
            impl Iter { Option Next() { return Option::None(); } }
            unit Main() { Iter iter = Iter { seed: 0 }; for i in iter { continue; } }
        ",
        expect_ok: false,
        error_matcher: Some(matches_iterable_option_some_arity),
    },
];

#[test]
fn typing_for_loop_iterable_errors() {
    for case in FOR_LOOP_ERROR_CASES {
        assert_type_case(case);
    }
}

const EVENT_HANDLER_TARGET_ERROR_CASES: &[TypeCase] = &[
    TypeCase {
        name: "add_assign_on_non_event",
        source: "type User { i64 count } unit Main() { mut User u = User { count: 0 }; unit(string) handler = (string payload) => { return; }; u.count += handler; }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_event_subscription),
    },
    TypeCase {
        name: "sub_assign_on_non_event",
        source: "type User { i64 count } unit Main() { mut User u = User { count: 0 }; unit(string) handler = (string payload) => { return; }; u.count -= handler; }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_event_subscription),
    },
];

#[test]
fn typing_rejects_event_handler_on_non_event_targets() {
    for case in EVENT_HANDLER_TARGET_ERROR_CASES {
        assert_type_case(case);
    }
}

const GENERIC_ERROR_CASES: &[TypeCase] = &[
    TypeCase {
        name: "missing_generic_args_for_call",
        source: "unit noop<T>() { } unit Main() { noop(); }",
        expect_ok: false,
        error_matcher: Some(matches_missing_type_args),
    },
    TypeCase {
        name: "generic_arg_conflict_for_call",
        source: "T pick<T>(T left, T right) { return left; } unit Main() { i64 x = pick(1, \"two\"); }",
        expect_ok: false,
        error_matcher: Some(matches_missing_type_args),
    },
    TypeCase {
        name: "generic_arg_mismatch_for_call",
        source: "T id<T>(T x) { return x; } unit Main() { i64 x = id<i64, string>(1); }",
        expect_ok: false,
        error_matcher: Some(matches_generic_arg_mismatch),
    },
    TypeCase {
        name: "missing_generic_args_for_type",
        source: "type Box<T> { T value } unit Main() { Box x = Box { value: 1 }; }",
        expect_ok: false,
        error_matcher: Some(matches_missing_type_args),
    },
    TypeCase {
        name: "generic_arg_mismatch_for_type",
        source: "type Box<T> { T value } unit Main() { Box<i64, string> x = Box<i64> { value: 1 }; }",
        expect_ok: false,
        error_matcher: Some(matches_generic_arg_mismatch),
    },
];

#[test]
fn typing_generic_argument_errors() {
    for case in GENERIC_ERROR_CASES {
        assert_type_case(case);
    }
}

#[test]
fn typing_allows_declared_conformance_argument_coercion() {
    assert_type_case(&TypeCase {
        name: "conformance_argument_coercion",
        source: "contract Service { i64 run(i64 x); } type Worker : Service { i64 base } impl Worker { i64 run(i64 x) { return this.base + x; } } i64 apply(Service s) { return s.run(1); } i64 Main() { Worker w = Worker { base: 41 }; return apply(w); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_method_dispatch_is_receiver_aware() {
    assert_type_case(&TypeCase {
        name: "receiver_aware_method_dispatch",
        source: "type A { i64 value } type B { i64 value } impl A { i64 Get() { return this.value; } } impl B { i64 Get() { i64 delta = 1; return this.value + delta; } } i64 Main() { A a = A { value: 20 }; B b = B { value: 21 }; return a.Get() + b.Get(); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_rejects_identity_equality_on_numeric_values() {
    assert_type_case(&TypeCase {
        name: "numeric_identity_equality",
        source: "bool Main() { return 1 === 1; }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_binary_op),
    });
}

#[test]
fn typing_allows_identity_equality_on_named_values() {
    assert_type_case(&TypeCase {
        name: "named_identity_equality",
        source: "type User { i64 id } bool Main() { User a = User { id: 1 }; User b = a; return a === b; }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_rejects_compound_assign_on_non_numeric_non_string() {
    assert_type_case(&TypeCase {
        name: "bool_compound_assign",
        source: "unit Main() { mut bool flag = true; flag += false; }",
        expect_ok: false,
        error_matcher: Some(matches_unsupported_expression),
    });
}

#[test]
fn typing_allows_string_compound_add_assign() {
    assert_type_case(&TypeCase {
        name: "string_compound_assign",
        source: "unit Main() { mut string s = \"a\"; s += \"b\"; }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_allows_event_member_subscribe_and_unsubscribe() {
    assert_type_case(&TypeCase {
        name: "event_subscribe_unsubscribe",
        source: "type User { event{4} Created(string payload) } unit Main() { mut User u = User { }; unit(string) handler = (string payload) => { return; }; u.Created += handler; u.Created -= handler; }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_rejects_zero_event_capacity() {
    assert_type_case(&TypeCase {
        name: "zero_event_capacity",
        source: "type User { event{0} Created(string payload) } unit Main() { return; }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_event_capacity),
    });
}

#[test]
fn typing_allows_owner_event_invoke() {
    assert_type_case(&TypeCase {
        name: "owner_event_invoke",
        source: "type User { event{4} Created(string payload) } impl User { unit Emit(string payload) { this.Created(payload); } } unit Main() { mut User u = User { }; u.Emit(\"ok\"); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_rejects_non_owner_event_invoke() {
    assert_type_case(&TypeCase {
        name: "non_owner_event_invoke",
        source: "type User { event{4} Created(string payload) } unit Main() { mut User u = User { }; u.Created(\"x\"); }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_event_invoke_scope),
    });
}

#[test]
fn typing_reports_unknown_method_call_target() {
    assert_type_case(&TypeCase {
        name: "unknown_method_call",
        source: "type Counter { i64 value } i64 Main() { Counter c = Counter { value: 1 }; return c.Missing(); }",
        expect_ok: false,
        error_matcher: Some(matches_unknown_call_target),
    });
}

#[test]
fn typing_literals_succeeds() {
    assert!(resolve_and_type("unit Main() { i64 x = 1; bool y = true; }").is_ok());
}

#[test]
fn typing_reports_mismatch() {
    assert_type_case(&TypeCase {
        name: "let_type_mismatch",
        source: "unit Main() { bool x = 1; }",
        expect_ok: false,
        error_matcher: Some(matches_type_mismatch),
    });
}

#[test]
fn typing_reports_non_bool_condition() {
    assert_type_case(&TypeCase {
        name: "non_bool_condition",
        source: "unit Main() { if 1 { i64 x = 1; } }",
        expect_ok: false,
        error_matcher: Some(matches_non_bool_condition),
    });
}

#[test]
fn typing_reports_return_mismatch() {
    assert_type_case(&TypeCase {
        name: "return_mismatch",
        source: "i64 Main() { return true; }",
        expect_ok: false,
        error_matcher: Some(matches_type_mismatch),
    });
}

#[test]
fn typing_function_calls_succeeds() {
    assert!(
        resolve_and_type(
            "i64 add(i64 a, i64 b) { return a + b; } unit Main() { i64 x = add(1, 2); }",
        )
        .is_ok()
    );
}

#[test]
fn typing_generic_function_call_succeeds() {
    assert_type_case(&TypeCase {
        name: "generic_call",
        source: "T id<T>(T x) { return x; } unit Main() { i64 x = id<i64>(1); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_generic_function_call_infers_from_arguments() {
    assert_type_case(&TypeCase {
        name: "generic_call_inferred",
        source: "T id<T>(T x) { return x; } unit Main() { i64 x = id(1); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_generic_equality_assertion_infers_from_arguments() {
    assert_type_case(&TypeCase {
        name: "generic_assert_equal_inferred",
        source: "unit AssertEqual<T>(T expected, T actual, string message) { if expected == actual { return; } } unit Main() { AssertEqual(1, 1, \"ok\"); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_generic_equality_assertion_infers_mixed_numeric_arguments() {
    assert_type_case(&TypeCase {
        name: "generic_assert_equal_mixed_numeric",
        source: "unit AssertEqual<T>(T expected, T actual, string message) { if expected == actual { return; } } unit Main() { i64 len = 3; AssertEqual(3, len, \"ok\"); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_reports_call_arity_mismatch() {
    assert_type_case(&TypeCase {
        name: "call_arity_mismatch",
        source: "i64 add(i64 a, i64 b) { return a + b; } unit Main() { i64 x = add(1); }",
        expect_ok: false,
        error_matcher: Some(matches_call_arity_mismatch),
    });
}

#[test]
fn typing_struct_literal_and_member_access() {
    assert_type_case(&TypeCase {
        name: "struct_literal_member",
        source: "type User { i64 id, string name } unit Main() { User u = User { id: 1, name: \"a\" }; i64 x = u.id; }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_path_expression_field_chain_resolves_nested_type() {
    assert_type_case(&TypeCase {
        name: "nested_path_field",
        source: "type Inner { i64 value } type Outer { Inner inner } unit Main() { Outer o = Outer { inner: Inner { value: 7 } }; i64 x = o.inner.value; }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_path_expression_method_dispatch_on_nested_receiver() {
    assert_type_case(&TypeCase {
        name: "nested_path_method",
        source: "type Inner { i64 value } impl Inner { i64 Get() { return this.value; } } type Outer { Inner inner } i64 Main() { Outer o = Outer { inner: Inner { value: 9 } }; return o.inner.Get(); }",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_reports_missing_struct_field() {
    assert_type_case(&TypeCase {
        name: "missing_struct_field",
        source: "type User { i64 id, string name } unit Main() { User u = User { id: 1 }; }",
        expect_ok: false,
        error_matcher: Some(matches_missing_struct_field),
    });
}

#[test]
fn typing_match_expression_unifies_types() {
    assert_type_case(&TypeCase {
        name: "match_unifies",
        source: "enum Choice { Some(string value), None } unit Main() { Choice opt = Choice::None(); string x = match opt { Choice::Some(value) => value, Choice::None => \"none\", }; }",
        expect_ok: true,
        error_matcher: None,
    });
}

const STRING_INTERPOLATION_OK_CASES: &[&str] = &[
    "unit Main() { string name = \"Ada\"; string msg = \"hi ${name}\"; }",
    "unit Main() { i64 code = 31; string msg = \"${code}\"; }",
    "unit Main() { string name = \"Ada\"; string suffix = \"!\"; string msg = \"hi ${name + suffix}\"; }",
];

#[test]
fn typing_string_interpolation_succeeds() {
    for source in STRING_INTERPOLATION_OK_CASES {
        assert!(
            resolve_and_type(source).is_ok(),
            "expected interpolation typing to succeed for: {source}"
        );
    }
}

#[test]
fn typing_records_cast_intent_for_numeric_mismatch() {
    let result = resolve_and_type("unit Main() { i32 x = 1; i64 y = x; }")
        .expect("expected typing to succeed with cast intent");
    assert_eq!(result.cast_intents.len(), 1);
    let intent = &result.cast_intents[0];
    assert_eq!(
        result.types.get(intent.from),
        Some(&TypeInfo::Primitive(HirPrimitiveType::I32))
    );
    assert_eq!(
        result.types.get(intent.to),
        Some(&TypeInfo::Primitive(HirPrimitiveType::I64))
    );
}

#[test]
fn typing_cast_intents_are_sorted_by_source_span() {
    let result = resolve_and_type("unit Main() { i32 a = 1; i64 b = a; i32 c = 2; i64 d = c; }")
        .expect("expected typing to succeed with cast intents");
    assert!(result.cast_intents.len() >= 2);
    for pair in result.cast_intents.windows(2) {
        assert!(pair[0].span.start <= pair[1].span.start);
    }
}

#[test]
fn typing_cast_intents_preserve_source_line_spans() {
    let result = resolve_and_type(
        "unit Main() {\n  i32 x = 1;\n  i64 y = x;\n  i32 z = 2;\n  i64 w = z;\n}",
    )
    .expect("expected typing to succeed with cast intents");
    let lines: Vec<usize> = result
        .cast_intents
        .iter()
        .map(|intent| intent.span.line_col_start.0)
        .collect();
    assert_eq!(lines, vec![3, 5]);
}

#[test]
fn typing_records_cast_intent_for_numeric_call_argument_mismatch() {
    let result = resolve_and_type(
        "i64 take(i64 v) { return v; } unit Main() { i32 x = 1; i64 y = take(x); }",
    )
    .expect("expected typing to succeed with cast intent in call argument");
    assert!(!result.cast_intents.is_empty());
}

#[test]
fn typing_records_cast_intent_for_numeric_return_mismatch() {
    let result = resolve_and_type("i64 Main() { i32 x = 1; return x; }")
        .expect("expected typing to succeed with cast intent in return");
    assert!(!result.cast_intents.is_empty());
}

#[test]
fn typing_cast_intent_accessor_finds_intent_by_span() {
    let result = resolve_and_type("unit Main() { i32 x = 1; i64 y = x; }")
        .expect("expected typing to succeed with cast intent");
    let span = result.cast_intents[0].span;
    assert!(result.cast_intent_for_span(span).is_some());
}

#[test]
fn typing_nested_match_expression_unifies_types() {
    assert_type_case(&TypeCase {
        name: "nested_match",
        source: "enum Choice { Some(i32 value), None } unit Main() { Choice x = Choice::Some(1); i32 y = match x { Choice::Some(v) => match x { Choice::Some(_) => v, Choice::None => 0, }, Choice::None => 0, }; }",
        expect_ok: true,
        error_matcher: None,
    });
}

const ENUM_PATTERN_ERROR_CASES: &[TypeCase] = &[
    TypeCase {
        name: "enum_pattern_arity_mismatch",
        source: "enum Choice { Some(i64 value), None } unit Main() { Choice x = Choice::Some(1); i64 y = match x { Choice::Some() => 0, Choice::None => 1, }; }",
        expect_ok: false,
        error_matcher: Some(matches_enum_constructor_mismatch),
    },
    TypeCase {
        name: "enum_pattern_field_type_mismatch",
        source: "enum Choice { Some(i64 value), None } unit Main() { Choice x = Choice::Some(1); i64 y = match x { Choice::Some(\"text\") => 0, Choice::None => 1, }; }",
        expect_ok: false,
        error_matcher: Some(matches_type_mismatch),
    },
];

#[test]
fn typing_enum_pattern_errors() {
    for case in ENUM_PATTERN_ERROR_CASES {
        assert_type_case(case);
    }
}

#[test]
fn typing_grouped_expression_propagates_type() {
    assert!(resolve_and_type("unit Main() { i64 x = (1); }").is_ok());
}

#[test]
fn typing_block_expression_propagates_unit_type() {
    assert!(resolve_and_type("unit Main() { unit x = { i64 y = 1; }; }").is_ok());
}

#[test]
fn typing_reports_invalid_member_target_for_non_struct() {
    assert_type_case(&TypeCase {
        name: "invalid_member_target",
        source: "unit Main() { i64 x = 1; i64 y = x.foo; }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_member_target),
    });
}

const ENUM_CONSTRUCTOR_ERROR_CASES: &[TypeCase] = &[
    TypeCase {
        name: "enum_constructor_arity_mismatch",
        source: "enum Choice { Some(i64 value), None } unit Main() { Choice x = Choice::Some(); }",
        expect_ok: false,
        error_matcher: Some(matches_enum_constructor_mismatch),
    },
    TypeCase {
        name: "bare_enum_constructor_arity_mismatch",
        source: "enum Choice { Some(i64 value), None } unit Main() { Choice x = Choice::Some; }",
        expect_ok: false,
        error_matcher: Some(matches_enum_constructor_mismatch),
    },
];

#[test]
fn typing_enum_constructor_errors() {
    for case in ENUM_CONSTRUCTOR_ERROR_CASES {
        assert_type_case(case);
    }
}

#[test]
fn typing_accepts_nullary_enum_constructor_without_parens() {
    assert!(
        resolve_and_type(
            "enum Choice { Some(i64 value), None } unit Main() { Choice x = Choice::None; }",
        )
        .is_ok()
    );
}

#[test]
fn typing_reports_unknown_struct_field() {
    assert_type_case(&TypeCase {
        name: "unknown_struct_field",
        source: "type User { i64 id, string name } unit Main() { User u = User { id: 1, name: \"a\" }; i64 x = u.age; }",
        expect_ok: false,
        error_matcher: Some(matches_unknown_struct_field),
    });
}

#[test]
fn typing_for_loop_infers_iterator_type_from_iterable_contract() {
    assert_type_case(&TypeCase {
        name: "iterable_for_loop",
        source: "
            enum Option { Some(i64 value), None }
            type Iter { i64 seed }
            impl Iter { Option Next() { return Option::Some(1); } }
            unit Main() { Iter iter = Iter { seed: 0 }; mut i64 sum = 0; for i in iter { sum += i; } }
        ",
        expect_ok: true,
        error_matcher: None,
    });
}

#[test]
fn typing_rejects_invalid_try_target() {
    assert_type_case(&TypeCase {
        name: "invalid_try_target",
        source: "i64 Main() { i64 x = 1; i64 y = x?; return y; }",
        expect_ok: false,
        error_matcher: Some(matches_invalid_try_target),
    });
}

#[test]
fn typing_allows_try_on_result_and_unwraps_ok_payload_type() {
    assert_type_case(&TypeCase {
        name: "try_on_result",
        source: "enum Result { Ok(i64 value), Error(string message) } i64 Main() { Result r = Result::Ok(42); i64 value = r?; return value; }",
        expect_ok: true,
        error_matcher: None,
    });
}
