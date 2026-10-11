//! Function-typed values: lambdas lowered to closure records and indirect calls through them.

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
