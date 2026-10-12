//! Function values: calls through function-typed fields and call results, named functions read
//! as values, and lambdas that must materialize their closure record.

use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    CallLowering, ManagedReferenceKind, SemanticTypeId, abi_type, call_lowering, direct_callees, function_item_value,
    function_value_call, lambda_value_required, managed_reference_kind,
};

use super::support::{assert_unavailable, key, setup};

#[test]
fn a_function_typed_field_projection_is_a_function_value_callee() {
    let source = "type Handler { (i64) => i64 op } i64 Main(Handler h) { return h.op(5); }";
    let (db, _project, unit, generation, index) = setup(source);
    let call = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let lowering = call_lowering(&db, call).expect("call lowering query").expect("call lowering");
    let CallLowering::FunctionValue(callee) = lowering else { panic!("{lowering:?}") };
    assert_eq!(callee, key(unit, generation, &index, NodeKind::PathExpression, 0), "the callee is the `h.op` projection");
    let fact = function_value_call(&db, call).expect("function value query").expect("function value");
    assert_eq!(fact.callee, callee);
    assert_eq!(fact.signature.parameters.as_ref(), &[SemanticTypeId::I64]);
    assert_eq!(fact.signature.result, SemanticTypeId::I64);
    assert_eq!(fact.result_managed_reference, ManagedReferenceKind::NativeOrScalar);
    assert_eq!(abi_type(&db, call).expect("call ABI query"), Some(SemanticTypeId::I64));
}

#[test]
fn a_call_result_of_function_type_is_a_function_value_callee_with_a_managed_result() {
    let source = "(i64) => string Make() { return (i64 x) => \"n\"; } string Main() { return Make()(1); }";
    let (db, _project, unit, generation, index) = setup(source);
    let outer = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let inner = key(unit, generation, &index, NodeKind::CallExpression, 1);
    assert!(matches!(call_lowering(&db, inner).expect("inner lowering"), Some(CallLowering::Direct(_))));
    let lowering = call_lowering(&db, outer).expect("outer lowering query").expect("outer lowering");
    assert!(matches!(lowering, CallLowering::FunctionValue(callee) if callee == inner), "{lowering:?}");
    let fact = function_value_call(&db, outer).expect("function value query").expect("function value");
    assert_eq!(fact.signature.result, SemanticTypeId::STRING);
    assert_eq!(fact.result_managed_reference, ManagedReferenceKind::GcManaged);
    assert_eq!(managed_reference_kind(&db, outer).expect("managed query"), Some(ManagedReferenceKind::GcManaged));
}

#[test]
fn a_named_function_read_as_a_value_has_a_record_abi_and_is_reachable() {
    let source = "i64 Double(i64 x) { return x * 2; } i64 Apply(i64 v, (i64) => i64 f) { return f(v); } \
                  i64 Main() { return Apply(4, Double); }";
    let (db, _project, unit, generation, index) = setup(source);
    let double = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 2);
    // Paths in order: `x` (Double), `f`, `v` (Apply), `Apply`, `Double` (Main).
    let value = key(unit, generation, &index, NodeKind::PathExpression, 4);
    let fact = function_item_value(&db, value).expect("function item query").expect("function item value");
    assert_eq!(fact.declaration, double);
    assert_eq!(fact.signature.parameters.as_ref(), &[SemanticTypeId::I64]);
    assert_eq!(abi_type(&db, value).expect("value ABI query"), Some(SemanticTypeId::POINTER));
    assert_eq!(managed_reference_kind(&db, value).expect("managed query"), Some(ManagedReferenceKind::GcManaged));
    assert!(direct_callees(&db, main).expect("callees query").expect("callees").contains(&double));
    // The call callee `Apply` is not a value, and a lexical local never is.
    assert_eq!(function_item_value(&db, key(unit, generation, &index, NodeKind::PathExpression, 3)).expect("callee"), None);
    assert_eq!(function_item_value(&db, key(unit, generation, &index, NodeKind::PathExpression, 0)).expect("local"), None);
}

#[test]
fn a_generic_function_read_as_a_value_is_unavailable() {
    let source = "T Same<T>(T x) { return x; } i64 Main() { (i64) => i64 f = Same; return f(1); }";
    let (db, _project, unit, generation, index) = setup(source);
    // Paths in order: `x` (Same), `Same`, `f` (Main).
    assert_unavailable(function_item_value(&db, key(unit, generation, &index, NodeKind::PathExpression, 1)));
}

#[test]
fn lambda_value_requirement_follows_the_use_of_the_lambda() {
    let source = "i64 Apply(i64 v, (i64) => i64 f) { return f(v); } \
                  i64 Main(i64 n) { let inline = (i64 x) => x + n; let passed = (i64 x) => x; \
                  return inline(1) + Apply(2, passed) + Apply(3, (i64 x) => x * n); }";
    let (db, _project, unit, generation, index) = setup(source);
    let required = |occurrence| {
        lambda_value_required(&db, key(unit, generation, &index, NodeKind::LambdaExpression, occurrence))
            .expect("lambda value query")
    };
    assert_eq!(required(0), Some(false), "an immutable lambda local used only as a callee stays inline");
    assert_eq!(required(1), Some(true), "a lambda local passed as a value materializes its record");
    assert_eq!(required(2), Some(true), "a lambda argument is a function value");
}
