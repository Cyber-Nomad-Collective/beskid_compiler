//! Function values: calls through function-typed fields and call results, named functions read
//! as values (generic ones instantiated where they are read), and lambdas that must materialize
//! their closure record.

use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    CallLowering, ManagedReferenceKind, SemanticTypeId, abi_type, call_lowering, check_items, direct_callees,
    function_item_value, function_value_call, function_value_call_specialization, generic_call_specialization,
    generic_call_specialization_instance, lambda_value_required, managed_reference_kind,
};

use super::support::{assert_unavailable, key, key_at_start, setup};

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

/// The path expression that starts at the first occurrence of `needle` in `source`.
fn path_at(
    source: &str,
    needle: &str,
    unit: beskid_queries::SourceUnitId,
    generation: beskid_queries::SyntaxGenerationId,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
) -> beskid_queries::AstNodeKey {
    let start = source.find(needle).unwrap_or_else(|| panic!("`{needle}` is in the source"));
    key_at_start(unit, generation, index, NodeKind::PathExpression, start)
}

/// The diagnostic codes the legality gate reports for `item`.
fn gate_codes(db: &beskid_queries::BeskidDatabase, item: beskid_queries::AstNodeKey) -> Vec<String> {
    match check_items(db, &[item]) {
        Ok(()) => Vec::new(),
        Err(findings) => findings.iter().map(|finding| finding.kind.code().to_string()).collect(),
    }
}

#[test]
fn a_generic_function_with_explicit_type_arguments_is_a_specialized_value() {
    let source = "T Identity<T>(T x) { return x; } i64 Apply(i64 v, (i64) => i64 f) { return f(v); } \
                  i64 Main() { return Apply(5, Identity<i64>); }";
    let (db, _project, unit, generation, index) = setup(source);
    let identity = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 2);
    let value = path_at(source, "Identity<i64>)", unit, generation, &index);
    let fact = function_item_value(&db, value).expect("function item query").expect("function item value");
    assert_eq!(fact.declaration, identity);
    assert_eq!(fact.signature.parameters.as_ref(), &[SemanticTypeId::I64]);
    assert_eq!(fact.signature.result, SemanticTypeId::I64);
    let instance = fact.specialization.expect("a generic value is a specialization");
    assert_eq!(instance.declaration, identity);
    assert_eq!(instance.substitutions.len(), 1);
    assert_eq!(instance.substitutions[0].parameter.as_ref(), "T");
    assert_eq!(instance.substitutions[0].argument, SemanticTypeId::I64);
    assert_eq!(abi_type(&db, value).expect("value ABI query"), Some(SemanticTypeId::POINTER));
    assert_eq!(managed_reference_kind(&db, value).expect("managed query"), Some(ManagedReferenceKind::GcManaged));
    assert!(direct_callees(&db, main).expect("callees query").expect("callees").contains(&identity));
    assert_eq!(gate_codes(&db, main), Vec::<String>::new());
}

#[test]
fn the_expected_function_type_fixes_the_type_arguments_of_a_generic_value() {
    let source = "T Same<T>(T x) { return x; } i64 Apply(i64 v, (i64) => i64 f) { return f(v); } \
                  i64 Main() { (i64) => i64 f = Same; return f(1) + Apply(2, Same); }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 2);
    for needle in ["Same; return", "Same); }"] {
        let value = path_at(source, needle, unit, generation, &index);
        let fact = function_item_value(&db, value).expect("function item query").expect("function item value");
        assert_eq!(fact.signature.parameters.as_ref(), &[SemanticTypeId::I64], "{needle}");
        assert_eq!(fact.signature.result, SemanticTypeId::I64, "{needle}");
        let instance = fact.specialization.expect("a generic value is a specialization");
        assert_eq!(instance.substitutions[0].argument, SemanticTypeId::I64, "{needle}");
    }
    assert_eq!(gate_codes(&db, main), Vec::<String>::new());
}

#[test]
fn the_explicit_type_arguments_of_a_generic_callee_type_a_generic_value_argument() {
    let source = "T Identity<T>(T x) { return x; } T Apply<T>(T v, (T) => T f) { return f(v); } \
                  i64 Main() { return Apply<i64>(5, Identity); }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 2);
    let value = path_at(source, "Identity); }", unit, generation, &index);
    let fact = function_item_value(&db, value).expect("function item query").expect("function item value");
    assert_eq!(fact.signature.parameters.as_ref(), &[SemanticTypeId::I64]);
    assert_eq!(gate_codes(&db, main), Vec::<String>::new());
}

#[test]
fn a_generic_value_without_type_arguments_or_expected_type_is_e1203() {
    let source = "T Same<T>(T x) { return x; } i64 Main() { let f = Same; return 1; }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert_unavailable(function_item_value(&db, path_at(source, "Same; return", unit, generation, &index)));
    assert_eq!(gate_codes(&db, main), vec!["E1203".to_string()]);
}

#[test]
fn a_generic_value_bound_to_two_types_by_its_expected_type_is_e1229() {
    let source = "T Same<T>(T x) { return x; } i64 Main() { (i64) => string f = Same; return 1; }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let error = function_item_value(&db, path_at(source, "Same; return", unit, generation, &index))
        .expect_err("the expected type binds `T` twice");
    assert!(error.binding_conflict().is_some(), "{error:?}");
    assert_eq!(gate_codes(&db, main), vec!["E1229".to_string()]);
}

#[test]
fn a_generic_value_with_the_wrong_type_argument_count_is_e1204() {
    let source = "T Same<T>(T x) { return x; } i64 Main() { (i64) => i64 f = Same<i64, i64>; return f(1); }";
    let (db, _project, unit, generation, index) = setup(source);
    let main = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert_eq!(gate_codes(&db, main), vec!["E1204".to_string()]);
}

#[test]
fn a_generic_value_typed_by_an_enclosing_generic_parameter_is_e1203() {
    let source = "T Same<T>(T x) { return x; } T Wrap<T>(T x) { (T) => T f = Same; return f(x); }";
    let (db, _project, unit, generation, index) = setup(source);
    let wrap = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    assert_eq!(gate_codes(&db, wrap), vec!["E1203".to_string()]);
}

#[test]
fn a_function_value_call_in_a_specialized_body_takes_its_result_from_the_instance() {
    let source = "T Twice<T>(T x, (T) => T f) { return f(f(x)); } \
                  string Main(string s) { i64 n = Twice<i64>(1, (i64 x) => x + 1); \
                  return Twice<string>(s, (string v) => v); }";
    let (db, _project, unit, generation, index) = setup(source);
    // Calls in order: `f(f(x))`, `f(x)` (Twice), `Twice<i64>(..)`, `Twice<string>(..)` (Main).
    let outer = key(unit, generation, &index, NodeKind::CallExpression, 0);
    let inner = key(unit, generation, &index, NodeKind::CallExpression, 1);
    let instance = |occurrence| {
        let call = key(unit, generation, &index, NodeKind::CallExpression, occurrence);
        let specialization =
            generic_call_specialization(&db, call).expect("specialization query").expect("generic call");
        generic_call_specialization_instance(&db, specialization).expect("instance query").expect("instance")
    };
    for (occurrence, result, managed) in [
        (2, SemanticTypeId::I64, ManagedReferenceKind::NativeOrScalar),
        (3, SemanticTypeId::STRING, ManagedReferenceKind::GcManaged),
    ] {
        let enclosing = instance(occurrence);
        for call in [outer, inner] {
            let fact = function_value_call_specialization(&db, call, &enclosing)
                .expect("specialized function value query")
                .expect("function value call");
            assert_eq!(fact.signature.parameters.as_ref(), &[result]);
            assert_eq!(fact.signature.result, result);
            assert_eq!(fact.result_managed_reference, managed);
        }
    }
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

#[test]
fn a_typed_lambda_parameter_is_classified_by_its_written_type() {
    // ParallelSuite `Parallel.Scope<bool>((Core.Parallel.Scopes.Scope s) => { .. })`: the lambda
    // trampoline needs each parameter's managed-reference class from its annotation.
    let source = "type Box { i64 v } i64 Run((Box, i64) => i64 f, Box b) { return f(b, 1); } \
                  i64 Main(Box b) { return Run((Box s, i64 n) => s.v + n, b); }";
    let (db, _project, unit, generation, index) = setup(source);
    let boxed = key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("s, i64 n").expect("s"));
    let scalar = key_at_start(unit, generation, &index, NodeKind::Identifier, source.find("n) =>").expect("n"));
    assert_eq!(managed_reference_kind(&db, boxed).expect("managed query"), Some(ManagedReferenceKind::GcManaged));
    assert_eq!(managed_reference_kind(&db, scalar).expect("managed query"), Some(ManagedReferenceKind::NativeOrScalar));
}
