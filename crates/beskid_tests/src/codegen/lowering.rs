use crate::codegen::util::lower_resolve_type;
use beskid_codegen::lowering::lower_program;

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_basic_function_to_clif() {
    let (hir, resolution, typed) = lower_resolve_type("i64 Main() { i64 x = 1; return x; }");
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected codegen lowering to succeed");
    assert_eq!(artifact.functions.len(), 1);
    let clif = artifact.functions[0].function.to_string();
    assert!(clif.contains("iconst"));
    assert!(clif.contains("return"));
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_spawn_expression() {
    let (hir, resolution, typed) =
        lower_resolve_type("i64 child() { return 42; } i64 Main() { spawn child; return 0; }");
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected spawn expression lowering to succeed");
    assert_eq!(artifact.functions.len(), 3, "expected child, main, and spawn entry trampoline");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_desugared_try_match() {
    let source = "enum Result { Ok(i64 value), Error(string message) } i64 Main() { Result r = Result::Ok(1); i64 value = r?; return value; }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected desugared try/match lowering to succeed");
    let main_fn = artifact.functions.iter().find(|f| f.name == "Main").expect("expected main function");
    let clif = main_fn.function.to_string();
    assert!(
        clif.contains("trap") && clif.contains("brif"),
        "expected try-expression control-flow/trap lowering in CLIF: {clif}"
    );
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_string_equality_via_str_eq() {
    let (hir, resolution, typed) = lower_resolve_type("bool Main() { return \"a\" == \"a\"; }");
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected string equality lowering");
    let clif = artifact.functions[0].function.to_string();
    assert!(
        clif.contains("interop_dispatch_i64"),
        "expected content-based string equality via str_eq dispatch, got: {clif}"
    );
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_numeric_cast_intent_via_sextend_or_ireduce() {
    let (hir, resolution, typed) = lower_resolve_type("i32 Main() { i64 x = 1; return x; }");
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected numeric cast intent to be supported without error");
    let clif = artifact.functions[0].function.to_string();
    assert!(clif.contains("ireduce.i32"), "expected i64->i32 reduction in CLIF: {clif}");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_range_for_loop_with_assignment() {
    let source = "i32 Main() { mut i32 sum = 0; for i in range(0, 4) { sum = sum + i; } return sum; }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected for loop lowering to succeed");
    let clif = artifact.functions[0].function.to_string();
    assert!(clif.contains("brif"), "expected loop branching in CLIF: {clif}");
    assert!(clif.contains("iadd"), "expected arithmetic increment/addition in CLIF: {clif}");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_generic_iterable_for_loop() {
    let source = "
        enum Option { Some(i64 value), None }
        type CounterIter { i64 sentinel }
        impl CounterIter {
            Option Next() {
                return Option::None();
            }
        }
        i64 Main() {
            CounterIter iter = CounterIter { sentinel: 0 };
            for i in iter {
                continue;
            }
            return 0;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected iterable for-loop lowering");
    let main = artifact.functions.iter().find(|f| f.name == "Main").expect("expected main function");
    let clif = main.function.to_string();
    assert!(clif.contains("brif"), "expected loop branching in CLIF: {clif}");
    assert!(clif.contains("jump"), "expected control-flow jumps in CLIF: {clif}");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_nullary_enum_constructor_without_parens() {
    let source = "
        enum Option { Some(i64 value), None }
        i64 Main() {
            Option value = Option::None;
            return 0;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected nullary enum constructor lowering");
    let main = artifact.functions.iter().find(|f| f.name == "Main").expect("expected main function");
    let clif = main.function.to_string();
    assert!(clif.contains("store"), "expected enum tag store in CLIF: {clif}");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_while_with_break_and_continue() {
    let source = "i32 Main() { mut i32 i = 0; mut i32 sum = 0; while i < 5 { i = i + 1; if i == 2 { continue; } if i == 4 { break; } sum = sum + i; } return sum; }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected while/break/continue lowering to succeed");
    let clif = artifact.functions[0].function.to_string();
    assert!(clif.contains("brif"), "expected branching in CLIF: {clif}");
    assert!(clif.contains("jump"), "expected jumps for loop control in CLIF: {clif}");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_functions_inside_inline_modules() {
    let source = "pub mod std { pub mod math { pub i64 one() { return 1; } } }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected module function lowering");

    assert_eq!(artifact.functions.len(), 1);
    assert_eq!(artifact.functions[0].name, "one");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_method_and_member_call() {
    let source = "type Counter { i64 value } impl Counter { i64 Get() { return this.value; } } i64 Main() { Counter c = Counter { value: 7 }; return c.Get(); }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected method lowering to succeed");

    assert!(artifact.functions.iter().any(|f| f.name == "__method__Counter__Get"), "expected lowered method symbol");
    assert!(artifact.functions.iter().any(|f| f.name == "Main"), "expected main function to be lowered");
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_contract_dispatch_via_indirect_call() {
    let source = "
        contract Service { i64 run(i64 x); }
        type Worker : Service { i64 base }
        impl Worker { i64 run(i64 x) { return this.base + x; } }
        i64 apply(Service s) { return s.run(1); }
        i64 Main() {
            Worker w = Worker { base: 41 };
            return apply(w);
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected contract dispatch lowering");

    let apply_fn = artifact.functions.iter().find(|f| f.name == "apply").expect("expected apply function");
    let apply_clif = apply_fn.function.to_string();
    assert!(
        apply_clif.contains("call_indirect"),
        "expected contract dispatch via indirect call in apply: {apply_clif}"
    );
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_event_subscribe_unsubscribe_and_invoke() {
    let source = "
        type User { event{4} Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        unit Main() {
            mut User u = User { };
            unit(string) handler = (string payload) => { return; };
            u.Created += handler;
            u.Emit(\"hello\");
            u.Created -= handler;
            return;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected event lifecycle lowering");

    let main_fn = artifact.functions.iter().find(|f| f.name == "Main").expect("expected main function");
    let main_clif = main_fn.function.to_string();
    assert!(
        main_clif.contains("interop_dispatch"),
        "expected event subscribe/unsubscribe via interop dispatch in main: {main_clif}"
    );

    let emit_fn =
        artifact.functions.iter().find(|f| f.name == "__method__User__Emit").expect("expected Emit method function");
    let emit_clif = emit_fn.function.to_string();
    assert!(
        emit_clif.contains("interop_dispatch") && emit_clif.contains("call_indirect"),
        "expected event invoke lowering via dispatch iteration and indirect calls: {emit_clif}"
    );
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_value_producing_match_returning_bool() {
    let source = "enum Result { Ok(i64 value), Error(i64 error) } \
        bool IsOk(Result value) { return match value { Result::Ok(_) => true, Result::Error(_) => false }; } \
        bool Main() { Result r = Result::Ok(1); return IsOk(r); }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed).expect("expected bool match return lowering");
    let is_ok = artifact.functions.iter().find(|f| f.name == "IsOk").expect("expected IsOk function");
    let clif = is_ok.function.to_string();
    assert!(
        clif.contains("return") && (clif.contains("iconst") || clif.contains("icmp")),
        "expected value-producing match to lower to bool return: {clif}"
    );
}

#[ignore = "HIR lower_program path incomplete after syntax-ISLE cutover; covered by syntax-ISLE / engine probes"]
#[test]
fn codegen_lowers_event_lifecycle_for_default_capacity_form() {
    let source = "
        type User { event Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        unit Main() {
            mut User u = User { };
            unit(string) handler = (string payload) => { return; };
            u.Created += handler;
            u.Emit(\"hello\");
            u.Created -= handler;
            return;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected default-capacity event lifecycle lowering");

    let main_fn = artifact.functions.iter().find(|f| f.name == "Main").expect("expected main function");
    let main_clif = main_fn.function.to_string();
    assert!(
        main_clif.contains("interop_dispatch"),
        "expected event subscribe/unsubscribe via interop dispatch in main: {main_clif}"
    );

    let emit_fn =
        artifact.functions.iter().find(|f| f.name == "__method__User__Emit").expect("expected Emit method function");
    let emit_clif = emit_fn.function.to_string();
    assert!(
        emit_clif.contains("interop_dispatch") && emit_clif.contains("call_indirect"),
        "expected event invoke lowering via dispatch iteration and indirect calls: {emit_clif}"
    );
}
