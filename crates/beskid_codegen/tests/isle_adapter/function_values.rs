//! Function-typed values: lambdas and named functions (generic ones through the specialization
//! selected where they are read) lowered to closure records, and indirect
//! calls through function-typed parameters, locals, fields and call results.

use super::support::{
    SyntaxModuleItem, find_function_definitions, find_nodes_of_kind, item_fixture_with_root, item_name,
    lower_syntax_program,
};

fn named(db: &dyn beskid_queries::Db, items: &[beskid_queries::AstNodeKey], name: &str) -> beskid_queries::AstNodeKey {
    items
        .iter()
        .copied()
        .find(|key| item_name(db, *key).ok().flatten().as_deref() == Some(name))
        .unwrap_or_else(|| panic!("function `{name}`"))
}

fn lowered_clif(artifact: &beskid_codegen::CodegenArtifact, symbol: &str) -> String {
    artifact
        .functions
        .iter()
        .find(|function| function.name == symbol)
        .unwrap_or_else(|| panic!("lowered function `{symbol}`"))
        .function
        .display()
        .to_string()
}

const APPLY: &str = "i64 Apply(i64 v, (i64) => i64 f) { return f(v); }\n";

#[test]
fn function_typed_parameter_has_the_closure_record_pointer_abi_and_an_indirect_call_fact() {
    let (input, isa, root) = item_fixture_with_root(APPLY);
    let db = input.database();
    let apply = find_function_definitions(db, root)[0];
    let signature = beskid_queries::item_abi_signature(db, apply).expect("Apply ABI query").expect("Apply ABI");
    assert_eq!(
        signature.parameters.as_ref(),
        &[beskid_queries::SemanticTypeId::I64, beskid_queries::SemanticTypeId::POINTER],
        "a function value is one pointer to its closure record"
    );
    assert_eq!(signature.result, beskid_queries::SemanticTypeId::I64);

    let call = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)[0];
    let lowering = beskid_queries::call_lowering(db, call).expect("call lowering query").expect("call lowering");
    assert!(matches!(lowering, beskid_queries::CallLowering::FunctionValue(_)), "{lowering:?}");
    let fact = beskid_queries::function_value_call(db, call).expect("function value query").expect("function value");
    assert_eq!(fact.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
    assert_eq!(fact.signature.result, beskid_queries::SemanticTypeId::I64);
    assert_eq!(fact.result_managed_reference, beskid_queries::ManagedReferenceKind::NativeOrScalar);
    let facts = beskid_codegen::SyntaxNodeFacts::new_with_isa(&input, isa.as_ref());
    assert_eq!(beskid_isle::NodeFacts::call_kind(&facts, call), Some(beskid_isle::CallKind::FunctionValue));

    let artifact =
        lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: apply, symbol: "Apply".into() }])
            .expect("a call through a function-typed parameter lowers");
    let clif = lowered_clif(&artifact, "Apply");
    // Code pointer and environment pointer are read from the record; the null-environment entry
    // takes only the source arguments, the capturing entry takes the environment first.
    assert!(clif.contains("+16]") || clif.contains("+16"), "{clif}");
    assert!(clif.contains("+24"), "{clif}");
    assert_eq!(clif.matches("call_indirect").count(), 2, "{clif}");
}

#[test]
fn capturing_lambda_argument_allocates_a_traced_closure_record() {
    let source = format!("{APPLY}i64 Main(string label, i64 offset) {{ return Apply(5, (i64 x) => x + offset); }}\n");
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (apply, main) = (named(db, &functions, "Apply"), named(db, &functions, "Main"));
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    assert_eq!(beskid_queries::lambda_value_required(db, lambda).expect("lambda value query"), Some(true));
    assert_eq!(
        beskid_queries::abi_type(db, lambda).expect("lambda ABI query"),
        Some(beskid_queries::SemanticTypeId::POINTER)
    );

    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: apply, symbol: "Apply".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a capturing lambda argument lowers to a closure record");
    assert!(artifact.event_handler_wrapper_required, "function values share the closure record layout");
    assert_eq!(artifact.closure_static_plans.len(), 1, "one environment descriptor for the capturing lambda");
    let clif = lowered_clif(&artifact, "Main");
    for needle in [
        "beskid_rt_v5_closure_environment_allocate",
        "beskid_rt_v5_managed_object_allocate",
        "__beskid_event_handler_allocation_request_v5",
        "func_addr",
        "gc_register_root",
    ] {
        assert!(clif.contains(needle), "missing `{needle}` in {clif}");
    }
    // The environment allocation is rooted before the record allocation that can collect.
    let environment = clif.find("beskid_rt_v5_closure_environment_allocate").expect("environment allocation");
    let record = clif.rfind("beskid_rt_v5_managed_object_allocate").expect("record allocation");
    let root = clif[environment..].find("gc_register_root").map(|offset| environment + offset).expect("env root");
    assert!(environment < root && root < record, "{clif}");
}

#[test]
fn managed_captures_are_in_the_environment_pointer_map() {
    let source = "string Call((string) => string f) { return f(\"!\"); }\n\
                  string Main(string prefix) { return Call((string s) => prefix + s); }\n";
    let (input, _isa, root) = item_fixture_with_root(source);
    let lambda = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    let plan = input.closure_static_plan(lambda).expect("capturing lambda static plan");
    assert_eq!(plan.captures.len(), 1);
    assert_eq!(plan.pointer_map_offsets.as_ref(), &[16], "a captured string is traced by the environment");
    assert_eq!(plan.captures[0].pointer_map_index, Some(0));
}

#[test]
fn returned_lambda_and_function_valued_result_lower() {
    let source = "(i64) => i64 MakeAdder(i64 n) { return (i64 x) => x + n; }\n\
                  i64 Main() { (i64) => i64 add = MakeAdder(3); return add(4); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (make, main) = (named(db, &functions, "MakeAdder"), named(db, &functions, "Main"));
    let signature = beskid_queries::item_abi_signature(db, make).expect("MakeAdder ABI query").expect("MakeAdder ABI");
    assert_eq!(signature.result, beskid_queries::SemanticTypeId::POINTER);
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: make, symbol: "MakeAdder".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("returning a capturing lambda lowers");
    assert!(lowered_clif(&artifact, "MakeAdder").contains("beskid_rt_v5_managed_object_allocate"));
    let main_clif = lowered_clif(&artifact, "Main");
    assert_eq!(main_clif.matches("call_indirect").count(), 2, "{main_clif}");
    assert!(main_clif.contains("gc_register_root"), "the function-valued local is a GC root: {main_clif}");
}

#[test]
fn lambda_local_used_only_as_a_callee_stays_inline_without_a_record() {
    let source = "i64 Main(i64 offset) { let add = (i64 x) => x + offset; return add(1); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let main = find_function_definitions(db, root)[0];
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    assert_eq!(beskid_queries::lambda_value_required(db, lambda).expect("lambda value query"), Some(false));
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("inline lambda local lowers");
    assert!(!artifact.event_handler_wrapper_required);
    let clif = lowered_clif(&artifact, "Main");
    assert!(!clif.contains("call_indirect") && !clif.contains("managed_object_allocate"), "{clif}");
}

#[test]
fn lambda_local_passed_as_a_value_materializes_its_record() {
    let source = format!("{APPLY}i64 Main() {{ let twice = (i64 x) => x * 2; return Apply(twice(1), twice); }}\n");
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (apply, main) = (named(db, &functions, "Apply"), named(db, &functions, "Main"));
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    assert_eq!(beskid_queries::lambda_value_required(db, lambda).expect("lambda value query"), Some(true));
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: apply, symbol: "Apply".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a lambda local used as a value lowers");
    assert!(lowered_clif(&artifact, "Main").contains("beskid_rt_v5_managed_object_allocate"));
}

#[test]
fn mutable_lambda_local_calls_through_its_current_record() {
    let source = "i64 Main() { mut (i64) => i64 op = (i64 x) => x + 1; op = ((i64 x) => x * 10); return op(1); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let main = find_function_definitions(db, root)[0];
    let call = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)[0];
    assert_eq!(beskid_queries::closure_call_target(db, call).expect("closure target query"), None);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a reassigned function-valued local lowers");
    assert_eq!(lowered_clif(&artifact, "Main").matches("call_indirect").count(), 2);
}

#[test]
fn unit_function_value_call_lowers_as_a_statement() {
    let source = "unit Each((i64) => unit f) { f(1); f(2); return; }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let each = find_function_definitions(input.database(), root)[0];
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: each, symbol: "Each".into() }])
        .expect("unit function value calls lower as statements");
    assert_eq!(lowered_clif(&artifact, "Each").matches("call_indirect").count(), 4);
}

#[test]
fn lambda_capturing_a_mutable_binding_is_rejected_with_e1233() {
    let source = "i64 Main() { mut i64 count = 1; let read = () => count; return read(); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let main = find_function_definitions(input.database(), root)[0];
    let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect_err("a mutable capture must not lower");
    let rendered = error.to_string();
    assert!(rendered.contains("E1233"), "{rendered}");
    assert!(rendered.contains("count"), "{rendered}");
    assert!(!rendered.contains("E2101") && !rendered.contains("MissingRuleOrFact"), "{rendered}");
}

#[test]
fn unit_block_lambda_argument_lowers_to_a_unit_entry() {
    let source = "unit Repeat(i64 count, (i64) => unit body) { body(count); return; }\n\
                  unit Main() { Repeat(3, (i64 i) => { i; return; }); return; }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (repeat, main) = (named(db, &functions, "Repeat"), named(db, &functions, "Main"));
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    let signature = beskid_queries::closure_signature(db, lambda).expect("closure query").expect("closure signature");
    assert_eq!(signature.callable.result, beskid_queries::SemanticTypeId::UNIT);
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: repeat, symbol: "Repeat".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .unwrap_or_else(|error| panic!("unit block lambda argument lowers: {}", error));
}

#[test]
fn block_bodied_lambda_returns_through_its_declared_result() {
    let source = format!(
        "{APPLY}i64 Main(i64 limit) {{ \
         return Apply(15, (i64 x) => {{ if x > limit {{ return limit; }} return x; }}); }}\n"
    );
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (apply, main) = (named(db, &functions, "Apply"), named(db, &functions, "Main"));
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    let signature = beskid_queries::closure_signature(db, lambda).expect("closure query").expect("closure signature");
    assert_eq!(signature.callable.result, beskid_queries::SemanticTypeId::I64, "declared parameter type result");
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: apply, symbol: "Apply".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .unwrap_or_else(|error| panic!("block-bodied lambda lowers: {}", error));
}

#[test]
fn generic_higher_order_function_specializes_its_function_value_call() {
    let source = "T Apply<T>(T value, (T) => T f) { return f(value); }\n\
                  i64 Main() { return Apply<i64>(5, (i64 x) => x + 1); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let main = named(db, &find_function_definitions(db, root), "Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .unwrap_or_else(|error| panic!("generic higher-order call lowers: {}", error));
    let specialized = artifact
        .functions
        .iter()
        .find(|function| function.function.display().to_string().contains("call_indirect"))
        .expect("the specialized Apply body calls its function value indirectly");
    assert!(specialized.name != "Main");
}

#[test]
fn function_value_call_results_typed_by_the_instance_are_values_in_a_specialized_body() {
    // `f(f(x))` passes one indirect call's `T` result to another, and `T once = f(x)` stores it;
    // both need the result type and managed category the instance binds `T` to.
    let source = "T Twice<T>(T x, (T) => T f) { return f(f(x)); }\n\
                  T Stored<T>(T x, (T) => T f) { T once = f(x); return f(once); }\n\
                  i64 Main(string label) {\n\
                      string twice = Twice<string>(label, (string s) => s + \"!\");\n\
                      string stored = Stored<string>(label, (string s) => s + \"?\");\n\
                      return Twice<i64>(1, (i64 x) => x + 1) + Stored<i64>(2, (i64 x) => x * 3);\n\
                  }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let main = named(db, &find_function_definitions(db, root), "Main");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .unwrap_or_else(|error| panic!("function value call results lower in specialized bodies: {error}"));
    let specialized = artifact
        .functions
        .iter()
        .filter(|function| {
            (function.name.starts_with("Twice#") || function.name.starts_with("Stored#"))
                && function.name.contains("#generic_")
        })
        .map(|function| function.function.display().to_string())
        .collect::<Vec<_>>();
    assert_eq!(specialized.len(), 4, "Twice and Stored for string and i64");
    for clif in &specialized {
        // Two calls through the record, each with its null- and captured-environment convention.
        assert_eq!(clif.matches("call_indirect").count(), 4, "{clif}");
    }
}

#[test]
fn named_function_read_as_a_value_is_a_closure_record_with_the_item_entry() {
    let source = format!("{APPLY}i64 Double(i64 x) {{ return x * 2; }}\ni64 Main() {{ return Apply(4, Double); }}\n");
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (apply, double, main) =
        (named(db, &functions, "Apply"), named(db, &functions, "Double"), named(db, &functions, "Main"));
    let value = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::PathExpression)
        .into_iter()
        .find(|path| matches!(beskid_queries::function_item_value(db, *path), Ok(Some(_))))
        .expect("the `Double` argument is a function item value");
    let fact = beskid_queries::function_item_value(db, value).expect("function item query").expect("function item");
    assert_eq!(fact.declaration, double);
    assert_eq!(fact.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
    assert_eq!(beskid_queries::abi_type(db, value).expect("value ABI query"), Some(beskid_queries::SemanticTypeId::POINTER));
    assert_eq!(
        beskid_queries::managed_reference_kind(db, value).expect("managed query"),
        Some(beskid_queries::ManagedReferenceKind::GcManaged)
    );
    assert!(
        beskid_queries::direct_callees(db, main).expect("callees").expect("callee list").contains(&double),
        "a function read as a value is reachable from its reader"
    );
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: apply, symbol: "Apply".into() },
            SyntaxModuleItem { key: double, symbol: "Double".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a named function passed as a value lowers");
    assert!(artifact.event_handler_wrapper_required);
    let clif = lowered_clif(&artifact, "Main");
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
    assert!(clif.contains("func_addr"), "{clif}");
    assert!(!clif.contains("closure_environment_allocate"), "a named function has a null environment: {clif}");
}

#[test]
fn generic_function_read_as_a_value_points_at_its_specialization() {
    // Explicit type arguments and the expected function type of the position both select the
    // `Identity<i64>` instance; one specialization is emitted and both records point at it.
    let source = format!(
        "{APPLY}T Identity<T>(T x) {{ return x; }}\n\
         i64 Main() {{ (i64) => i64 f = Identity; return f(1) + Apply(5, Identity<i64>); }}\n"
    );
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (apply, identity, main) =
        (named(db, &functions, "Apply"), named(db, &functions, "Identity"), named(db, &functions, "Main"));
    let values = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::PathExpression)
        .into_iter()
        .filter_map(|path| beskid_queries::function_item_value(db, path).ok().flatten())
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2, "both reads of `Identity` are function values: {values:?}");
    for value in &values {
        assert_eq!(value.declaration, identity);
        assert_eq!(value.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
        let instance = value.specialization.as_ref().expect("a generic value is a specialization");
        assert_eq!(instance.substitutions[0].argument, beskid_queries::SemanticTypeId::I64);
    }
    assert_eq!(values[0].specialization, values[1].specialization, "one instance for one type argument list");
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[SyntaxModuleItem { key: apply, symbol: "Apply".into() }, SyntaxModuleItem { key: main, symbol: "Main".into() }],
    )
    .expect("a generic function read as a value lowers");
    let specialized = artifact
        .functions
        .iter()
        .filter(|function| function.name.contains("Identity") && function.name.contains("#generic_"))
        .count();
    assert_eq!(specialized, 1, "exactly one `Identity<i64>` specialization is emitted");
    let clif = lowered_clif(&artifact, "Main");
    assert_eq!(clif.matches("func_addr").count(), 2, "{clif}");
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
}

#[test]
fn generic_function_value_without_type_arguments_stops_at_the_legality_gate() {
    let cases = [
        ("T Same<T>(T x) { return x; }\ni64 Main() { let f = Same; return 1; }\n", "E1203"),
        ("T Same<T>(T x) { return x; }\ni64 Main() { (i64) => string f = Same; return 1; }\n", "E1229"),
    ];
    for (source, code) in cases {
        let (input, isa, root) = item_fixture_with_root(source);
        let db = input.database();
        let main = named(db, &find_function_definitions(db, root), "Main");
        let error = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
            .expect_err("a generic function value without an instance must not lower");
        let rendered = error.to_string();
        assert!(rendered.contains(code), "{source}: {rendered}");
        assert!(!rendered.contains("MissingRuleOrFact"), "{source}: {rendered}");
    }
}

#[test]
fn function_typed_struct_field_is_called_through_its_record() {
    let source = "type Handler { (i64) => i64 op }\n\
                  i64 Main(i64 n) { Handler h = Handler { op: (i64 x) => x + n }; return h.op(5); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let main = named(db, &find_function_definitions(db, root), "Main");
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    assert_eq!(beskid_queries::lambda_value_required(db, lambda).expect("lambda value query"), Some(true));
    let expected = beskid_queries::expected_type(db, lambda).expect("expected type query").expect("field expectation");
    assert!(matches!(expected.context, beskid_queries::ExpectedTypeContext::StructField { .. }), "{expected:?}");
    let call = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)[0];
    let lowering = beskid_queries::call_lowering(db, call).expect("call lowering query").expect("call lowering");
    assert!(matches!(lowering, beskid_queries::CallLowering::FunctionValue(_)), "{lowering:?}");
    let fact = beskid_queries::function_value_call(db, call).expect("function value query").expect("function value");
    assert_eq!(fact.signature.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
    assert_eq!(fact.signature.result, beskid_queries::SemanticTypeId::I64);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &[SyntaxModuleItem { key: main, symbol: "Main".into() }])
        .expect("a call through a function-typed field lowers");
    let clif = lowered_clif(&artifact, "Main");
    assert_eq!(clif.matches("call_indirect").count(), 2, "{clif}");
    assert!(clif.contains("beskid_rt_v5_managed_object_allocate"), "{clif}");
}

#[test]
fn call_on_a_call_result_lowers_through_the_returned_record() {
    let source = "(i64) => i64 Make() { return (i64 x) => x + 1; }\ni64 Main() { return Make()(1); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (make, main) = (named(db, &functions, "Make"), named(db, &functions, "Main"));
    let outer = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::CallExpression)
        .into_iter()
        .find(|call| {
            matches!(beskid_queries::call_lowering(db, *call), Ok(Some(beskid_queries::CallLowering::FunctionValue(_))))
        })
        .expect("`Make()(1)` is a call through a function value");
    let fact = beskid_queries::function_value_call(db, outer).expect("function value query").expect("function value");
    assert_eq!(fact.signature.result, beskid_queries::SemanticTypeId::I64);
    assert_eq!(beskid_queries::abi_type(db, outer).expect("call ABI query"), Some(beskid_queries::SemanticTypeId::I64));
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: make, symbol: "Make".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a call on a call result lowers");
    let clif = lowered_clif(&artifact, "Main");
    assert_eq!(clif.matches("call_indirect").count(), 2, "{clif}");
    assert!(clif.contains("gc_register_root"), "the returned record is rooted across the arguments: {clif}");
}

#[test]
fn block_bodied_lambda_literal_returns_take_the_declared_u8_result() {
    let source = "u8 Pick(u8 v, (u8) => u8 f) { return f(v); }\n\
                  u8 Main(u8 limit) { return Pick(9, (u8 x) => { if x > limit { return 2; } return 3; }); }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (pick, main) = (named(db, &functions, "Pick"), named(db, &functions, "Main"));
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    let signature = beskid_queries::closure_signature(db, lambda).expect("closure query").expect("closure signature");
    assert_eq!(signature.callable.result, beskid_queries::SemanticTypeId::U8);
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: pick, symbol: "Pick".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .unwrap_or_else(|error| panic!("u8 block lambda lowers: {}", error));
}

#[test]
fn untyped_lambda_parameters_take_the_callee_parameter_types() {
    let source = format!("{APPLY}i64 Main() {{ return Apply(5, x => x + 1); }}\n");
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (apply, main) = (named(db, &functions, "Apply"), named(db, &functions, "Main"));
    let lambda = find_nodes_of_kind(db, root, beskid_queries::IndexedNodeKind::LambdaExpression)[0];
    let signature = beskid_queries::closure_signature(db, lambda).expect("closure query").expect("closure signature");
    assert_eq!(signature.callable.parameters.as_ref(), &[beskid_queries::SemanticTypeId::I64]);
    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: apply, symbol: "Apply".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .unwrap_or_else(|error| panic!("untyped lambda argument lowers: {}", error));
}

#[test]
fn generic_struct_literal_with_an_unsuffixed_literal_field_lowers() {
    let source = "type Bag<T> { T value, i64 size }\n\
                  Bag<T> Make<T>(T value) { return Bag<T> { value: value, size: 1 }; }\n\
                  Bag<i64> Direct() { return Bag<i64> { value: 2, size: 3 }; }\n\
                  i64 Main() { Bag<i64> bag = Make<i64>(7_i64); Bag<i64> direct = Direct(); return bag.size + direct.value; }\n";
    let (input, isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let functions = find_function_definitions(db, root);
    let (make, direct, main) =
        (named(db, &functions, "Make"), named(db, &functions, "Direct"), named(db, &functions, "Main"));
    let artifact = lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: make, symbol: "Make".into() },
            SyntaxModuleItem { key: direct, symbol: "Direct".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .unwrap_or_else(|error| panic!("generic struct literals with bare literal fields lower: {}", error));
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Make#generic_")),
        "Make specializes: {:?}",
        artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
    );
}
