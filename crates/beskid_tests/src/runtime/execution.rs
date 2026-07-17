use crate::support::runtime::{
    aot_compile_only, aot_run_main_i32, aot_run_main_i64, build_aot_exe, validate_lowered,
};

#[test]
fn aot_compiles_simple_function() {
    let source = "i64 Main() { return 1; }";
    aot_compile_only(source);
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_array_new_builtin_call() {
    let source = "i64 Main() { return __array_new(8, 3); }";
    let value = aot_run_main_i64(source);
    assert_ne!(
        value, 0,
        "expected array_new to return non-null pointer value"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_string_len_builtin_call() {
    let source = "i64 Main() { return __str_len(\"hello\"); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 5,
        "expected string length builtin to return byte length"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_struct_allocation_and_returns_field() {
    let source =
        "type Boxed { i64 value } i64 Main() { Boxed b = Boxed { value: 41 }; return b.value; }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 41, "expected struct field value to round-trip");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_compiles_std_panic_builtin_call() {
    let source = "unit Main() { if false { __panic_str(\"boom\"); } }";
    aot_compile_only(source);
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_enum_allocation_and_returns_payload_field() {
    let source = "enum Choice { Some(i32 value), None } i32 Main() { Choice c = Choice::Some(7); i32 result = match c { Choice::Some(v) => v, Choice::None => 0, }; return result; }";
    let value = aot_run_main_i32(source);
    assert_eq!(value, 7, "expected enum payload field to round-trip");
}

#[test]
fn aot_linked_executable_is_produced() {
    let source = "i64 Main() { return 2; }";
    let (dir, result) = build_aot_exe(source, "aot_linked_exe");
    assert!(
        result.exe_path.exists(),
        "expected linked executable for simple main"
    );
    assert_eq!(result.exit_code, 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_spawn_expression() {
    let source = "i64 child() { return 42; } i64 Main() { spawn child; return 5; }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 5,
        "expected spawned child to run without corrupting main"
    );
}

#[test]
fn aot_compiles_syscall_write_builtin_call() {
    let source = "i64 Main() { return __syscall_write(1, \"hello\"); }";
    validate_lowered(source);
}

#[test]
fn aot_compiles_syscall_read_builtin_call() {
    let source = "string Main() { return __syscall_read(99, 8); }";
    validate_lowered(source);
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_local_lambda_call() {
    let source = "i64 Main() { let add = (i64 x, i64 y) => x + y; return add(20, 22); }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 42, "expected local lambda to be callable");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_closure_capture_call() {
    let source = "i64 Main() { i64 base = 41; let inc = (i64 x) => x + base; return inc(1); }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 42, "expected lambda closure to capture outer local");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_passes_lambda_as_argument_to_lambda() {
    let source = "i64 Main() { let apply = (i64(i64) f, i64 x) => f(x); let id = (i64 n) => n; return apply(id, 42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 42, "expected lambda argument passing to work");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_grouped_immediate_lambda_call() {
    let source = "i64 Main() { return ((i64 x) => x)(42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 42, "expected grouped lambda immediate call to work");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_passes_inline_lambda_argument() {
    let source =
        "i64 Main() { let apply = (i64(i64) f, i64 x) => f(x); return apply((i64 n) => n, 42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 42, "expected inline lambda argument passing to work");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_passes_inline_lambda_to_named_function() {
    let source = "i64 apply(i64(i64) f, i64 x) { return f(x); } i64 Main() { return apply((i64 n) => n, 42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected named function to call inline lambda argument"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_passes_local_lambda_to_named_function() {
    let source = "i64 apply(i64(i64) f, i64 x) { return f(x); } i64 Main() { let inc = (i64 n) => n; return apply(inc, 42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected named function to call local lambda argument"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_calls_function_typed_member_value() {
    let source = "type Holder { i64(i64) f } i64 Main() { Holder h = Holder { f: (i64 n) => n }; return h.f(42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(value, 42, "expected function-typed member call to work");
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_infers_lambda_parameter_type_from_typed_let() {
    let source = "i64 Main() { i64(i64) id = (n) => n; return id(42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected lambda parameter type inference from typed let"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_infers_lambda_parameter_type_from_named_function_argument() {
    let source =
        "i64 apply(i64(i64) f, i64 x) { return f(x); } i64 Main() { return apply((n) => n, 42); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected lambda parameter type inference from function argument"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_executes_method_call_with_this_field_access() {
    let source = "type Counter { i64 value } impl Counter { i64 Get() { return this.value; } } i64 Main() { Counter c = Counter { value: 42 }; return c.Get(); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected method call to read receiver field via this"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_dispatches_same_method_name_by_receiver_type() {
    let source = "type A { i64 value } type B { i64 value } impl A { i64 Get() { return this.value; } } impl B { i64 Get() { i64 delta = 1; return this.value + delta; } } i64 Main() { A a = A { value: 20 }; B b = B { value: 21 }; return a.Get() + b.Get(); }";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected receiver-specific method dispatch to call matching method body"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_event_invoke_executes_subscribed_handler() {
    let source = "
        type User { event{4} Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        i64 Main() {
            mut User u = User { };
            unit(string) handler = (string payload) => { __syscall_write(1, payload); };
            u.Created += handler;
            u.Emit(\"x\");
            return 42;
        }
    ";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected AOT event invoke path to execute successfully"
    );
}

#[ignore = "AOT/HIR runtime probes incomplete on local ABI-v5 kit (missing _alloc / expression types / try desugar)"]
#[test]
fn aot_event_unsubscribe_removes_first_match() {
    let source = "
        type User { event{4} Created(string payload) }
        impl User {
            unit Emit(string payload) { this.Created(payload); }
        }
        i64 Main() {
            mut User u = User { };
            unit(string) boom = (string payload) => { __panic_str(\"boom\"); };
            u.Created += boom;
            u.Created -= boom;
            u.Emit(\"x\");
            return 42;
        }
    ";
    let value = aot_run_main_i64(source);
    assert_eq!(
        value, 42,
        "expected first-match unsubscribe to remove handler"
    );
}
