use std::collections::HashMap;

use crate::resolve::ItemId;
use crate::syntax::PrimitiveType;
use crate::syntax::SpanInfo;
use crate::types::inference::{
    ConstraintSet, FunctionSignature, InferenceResult, TypeEnv, TypeVar, infer_generic_args_from_call_types,
    is_numeric, solve_constraints, unify_numeric_types, unify_types,
};
use crate::types::result::TypeError;
use crate::types::{TypeId, TypeInfo, TypeTable};

fn i64_id(table: &mut TypeTable) -> TypeId {
    table.intern(TypeInfo::Primitive(PrimitiveType::I64))
}

fn i32_id(table: &mut TypeTable) -> TypeId {
    table.intern(TypeInfo::Primitive(PrimitiveType::I32))
}

fn bool_id(table: &mut TypeTable) -> TypeId {
    table.intern(TypeInfo::Primitive(PrimitiveType::Bool))
}

fn generic_param(table: &mut TypeTable, name: &str) -> TypeId {
    table.intern(TypeInfo::GenericParam(name.to_string()))
}

#[test]
fn unify_numeric_prefers_i64_when_either_side_is_i64() {
    let mut table = TypeTable::new();
    let left = i32_id(&mut table);
    let right = i64_id(&mut table);
    let unified = unify_types(&table, left, right, SpanInfo::default()).expect("numeric unify");
    assert_eq!(unified, i64_id(&mut table));
}

#[test]
fn solve_equal_binds_type_var() {
    let mut table = TypeTable::new();
    let i64 = i64_id(&mut table);
    let mut set = ConstraintSet::default();
    let var = set.fresh_var();
    set.equal(var, i64, SpanInfo::default());
    let env = TypeEnv::new(&table);
    let result = solve_constraints(set, &env, SpanInfo::default()).expect("solve");
    assert_eq!(result.bindings.get(&var), Some(&i64));
}

#[test]
fn solve_is_numeric_unbound_is_ambiguous_e1202() {
    let table = TypeTable::new();
    let mut set = ConstraintSet::default();
    let var = set.fresh_var();
    set.is_numeric(var, SpanInfo::default(), "sum");
    let env = TypeEnv::new(&table);
    let errors = solve_constraints(set, &env, SpanInfo::default()).expect_err("ambiguous");
    assert!(errors.iter().any(|e| matches!(e, TypeError::MissingTypeAnnotation { name, .. } if name == "sum")));
}

#[test]
fn infer_generic_args_widens_numeric_bindings() {
    let mut table = TypeTable::new();
    let item = ItemId(2);
    let t = generic_param(&mut table, "T");
    let i32 = i32_id(&mut table);
    let i64 = i64_id(&mut table);

    let mut generic_items = HashMap::new();
    generic_items.insert(item, vec!["T".to_string()]);

    let mut function_signatures = HashMap::new();
    function_signatures.insert(item, FunctionSignature { params: vec![t, t], return_type: t });

    let inferred = infer_generic_args_from_call_types(&table, &generic_items, &function_signatures, item, &[i32, i64])
        .expect("infer widened T");
    assert_eq!(inferred, vec![i64]);
}

#[test]
fn solve_apply_generic_binds_result_vars() {
    let mut table = TypeTable::new();
    let item = ItemId(3);
    let t = generic_param(&mut table, "T");
    let i64 = i64_id(&mut table);

    let mut generic_items = HashMap::new();
    generic_items.insert(item, vec!["T".to_string()]);

    let mut function_signatures = HashMap::new();
    function_signatures.insert(item, FunctionSignature { params: vec![t], return_type: t });

    let mut set = ConstraintSet::default();
    let result = set.fresh_var();
    set.apply_generic(item, vec![i64], vec![result], SpanInfo::default());

    let env = TypeEnv::new(&table).with_generics(&generic_items, &function_signatures);
    let solved = solve_constraints(set, &env, SpanInfo::default()).expect("apply generic");
    assert_eq!(solved.bindings.get(&result), Some(&i64));
}

#[test]
fn inference_result_resolve_helper() {
    let mut bindings = HashMap::new();
    let var = TypeVar(0);
    let ty = TypeId(0);
    bindings.insert(var, ty);
    let result = InferenceResult { bindings };
    assert_eq!(result.resolve(var), Some(ty));
}

#[test]
fn is_numeric_recognizes_numeric_primitives() {
    let mut table = TypeTable::new();
    let i32 = i32_id(&mut table);
    let i64 = i64_id(&mut table);
    let bool_ty = bool_id(&mut table);
    assert!(is_numeric(&table, i32));
    assert!(is_numeric(&table, i64));
    assert!(!is_numeric(&table, bool_ty));
}

#[test]
fn solve_variant_of_binds_enum_type() {
    let mut table = TypeTable::new();
    let enum_item = ItemId(4);
    let enum_type = table.intern(TypeInfo::Named(enum_item));

    let mut enum_variants = HashMap::new();
    enum_variants.insert(enum_item, HashMap::from([("Some".to_string(), vec![TypeId(0)])]));
    let mut named_types = HashMap::new();
    named_types.insert(enum_item, enum_type);

    let mut set = ConstraintSet::default();
    let var = set.fresh_var();
    set.variant_of(var, enum_item, "Some", SpanInfo::default());

    let env = TypeEnv::new(&table).with_enum_variants(&enum_variants).with_named_types(&named_types);
    let result = solve_constraints(set, &env, SpanInfo::default()).expect("variant solve");
    assert_eq!(result.bindings.get(&var), Some(&enum_type));
}

#[test]
fn unify_numeric_types_widens_i32_with_i64() {
    let mut table = TypeTable::new();
    let i32 = i32_id(&mut table);
    let i64 = i64_id(&mut table);
    assert_eq!(unify_numeric_types(&table, i32, i64), Some(i64));
}
