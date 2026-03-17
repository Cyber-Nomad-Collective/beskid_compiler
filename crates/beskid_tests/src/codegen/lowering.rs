use crate::codegen::util::lower_resolve_type;
use beskid_codegen::errors::CodegenError;
use beskid_codegen::lowering::lower_program;

#[test]
fn codegen_lowers_basic_function_to_clif() {
    let (hir, resolution, typed) = lower_resolve_type("i64 main() { i64 x = 1; return x; }");
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected codegen lowering to succeed");
    assert_eq!(artifact.functions.len(), 1);
    let clif = artifact.functions[0].function.to_string();
    assert!(clif.contains("iconst"));
    assert!(clif.contains("return"));
}

#[test]
fn codegen_rejects_unsupported_expression_nodes_with_span() {
    let (hir, resolution, typed) =
        lower_resolve_type("i64 main() { return match 1 { 1 => 2, _ => 3, }; }");
    let errors = lower_program(&hir, &resolution, &typed)
        .expect_err("expected unsupported match node to fail codegen");
    assert!(
        errors
            .iter()
            .any(|error| matches!(error, CodegenError::UnsupportedNode { .. })),
        "expected UnsupportedNode error, got: {errors:?}"
    );
}

#[test]
fn codegen_lowers_desugared_try_match() {
    let source = "enum Result { Ok(i64 value), Error(string message) } i64 main() { Result r = Result::Ok(1); i64 value = r?; return value; }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed)
        .expect("expected desugared try/match lowering to succeed");
    let main_fn = artifact
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("expected main function");
    let clif = main_fn.function.to_string();
    assert!(
        clif.contains("trap") && clif.contains("brif"),
        "expected try-expression control-flow/trap lowering in CLIF: {clif}"
    );
}

#[test]
fn codegen_lowers_numeric_cast_intent_via_sextend_or_ireduce() {
    let (hir, resolution, typed) = lower_resolve_type("i32 main() { i64 x = 1; return x; }");
    let artifact = lower_program(&hir, &resolution, &typed)
        .expect("expected numeric cast intent to be supported without error");
    let clif = artifact.functions[0].function.to_string();
    assert!(
        clif.contains("ireduce.i32"),
        "expected i64->i32 reduction in CLIF: {clif}"
    );
}

#[test]
fn codegen_lowers_range_for_loop_with_assignment() {
    let source = "i32 main() { i32 mut sum = 0; for i in range(0, 4) { sum = sum + i; } return sum; }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected for loop lowering to succeed");
    let clif = artifact.functions[0].function.to_string();
    assert!(
        clif.contains("brif"),
        "expected loop branching in CLIF: {clif}"
    );
    assert!(
        clif.contains("iadd"),
        "expected arithmetic increment/addition in CLIF: {clif}"
    );
}

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
        i64 main() {
            CounterIter iter = CounterIter { sentinel: 0 };
            for i in iter {
                continue;
            }
            return 0;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected iterable for-loop lowering");
    let main = artifact
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("expected main function");
    let clif = main.function.to_string();
    assert!(clif.contains("brif"), "expected loop branching in CLIF: {clif}");
    assert!(clif.contains("jump"), "expected control-flow jumps in CLIF: {clif}");
}

#[test]
fn codegen_lowers_while_with_break_and_continue() {
    let source = "i32 main() { i32 mut i = 0; i32 mut sum = 0; while i < 5 { i = i + 1; if i == 2 { continue; } if i == 4 { break; } sum = sum + i; } return sum; }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed)
        .expect("expected while/break/continue lowering to succeed");
    let clif = artifact.functions[0].function.to_string();
    assert!(clif.contains("brif"), "expected branching in CLIF: {clif}");
    assert!(
        clif.contains("jump"),
        "expected jumps for loop control in CLIF: {clif}"
    );
}

#[test]
fn codegen_lowers_functions_inside_inline_modules() {
    let source = "pub mod std { pub mod math { pub i64 one() { return 1; } } }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected module function lowering");

    assert_eq!(artifact.functions.len(), 1);
    assert_eq!(artifact.functions[0].name, "one");
}

#[test]
fn codegen_lowers_method_and_member_call() {
    let source = "type Counter { i64 value } impl Counter { i64 Get() { return this.value; } } i64 main() { Counter c = Counter { value: 7 }; return c.Get(); }";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected method lowering to succeed");

    assert!(
        artifact
            .functions
            .iter()
            .any(|f| f.name == "__method__Counter__Get"),
        "expected lowered method symbol"
    );
    assert!(
        artifact.functions.iter().any(|f| f.name == "main"),
        "expected main function to be lowered"
    );
}

#[test]
fn codegen_lowers_contract_dispatch_via_indirect_call() {
    let source = "
        contract Service { i64 run(i64 x); }
        type Worker : Service { i64 base }
        impl Worker { i64 run(i64 x) { return this.base + x; } }
        i64 apply(Service s) { return s.run(1); }
        i64 main() {
            Worker w = Worker { base: 41 };
            return apply(w);
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected contract dispatch lowering");

    let apply_fn = artifact
        .functions
        .iter()
        .find(|f| f.name == "apply")
        .expect("expected apply function");
    let apply_clif = apply_fn.function.to_string();
    assert!(
        apply_clif.contains("call_indirect"),
        "expected contract dispatch via indirect call in apply: {apply_clif}"
    );
}

#[test]
fn codegen_lowers_event_subscribe_unsubscribe_and_invoke() {
    let source = "
        type User { event{4} Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        unit main() {
            User mut u = User { };
            unit(string) handler = (string payload) => { return; };
            u.Created += handler;
            u.Emit(\"hello\");
            u.Created -= handler;
            return;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact =
        lower_program(&hir, &resolution, &typed).expect("expected event lifecycle lowering");

    let main_fn = artifact
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("expected main function");
    let main_clif = main_fn.function.to_string();
    assert!(
        main_clif.contains("event_subscribe") && main_clif.contains("event_unsubscribe_first"),
        "expected event subscribe/unsubscribe helper calls in main: {main_clif}"
    );

    let emit_fn = artifact
        .functions
        .iter()
        .find(|f| f.name == "__method__User__Emit")
        .expect("expected Emit method function");
    let emit_clif = emit_fn.function.to_string();
    assert!(
        emit_clif.contains("event_len") && emit_clif.contains("event_get_handler") && emit_clif.contains("call_indirect"),
        "expected event invoke lowering via helper iteration and indirect calls: {emit_clif}"
    );
}

#[test]
fn codegen_lowers_event_lifecycle_for_default_capacity_form() {
    let source = "
        type User { event Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        unit main() {
            User mut u = User { };
            unit(string) handler = (string payload) => { return; };
            u.Created += handler;
            u.Emit(\"hello\");
            u.Created -= handler;
            return;
        }
    ";
    let (hir, resolution, typed) = lower_resolve_type(source);
    let artifact = lower_program(&hir, &resolution, &typed)
        .expect("expected default-capacity event lifecycle lowering");

    let main_fn = artifact
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("expected main function");
    let main_clif = main_fn.function.to_string();
    assert!(
        main_clif.contains("event_subscribe") && main_clif.contains("event_unsubscribe_first"),
        "expected event subscribe/unsubscribe helper calls in main: {main_clif}"
    );

    let emit_fn = artifact
        .functions
        .iter()
        .find(|f| f.name == "__method__User__Emit")
        .expect("expected Emit method function");
    let emit_clif = emit_fn.function.to_string();
    assert!(
        emit_clif.contains("event_len")
            && emit_clif.contains("event_get_handler")
            && emit_clif.contains("call_indirect"),
        "expected event invoke lowering via helper iteration and indirect calls: {emit_clif}"
    );
}
