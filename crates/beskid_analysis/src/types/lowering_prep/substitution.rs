use std::collections::HashMap;

use crate::hir::HirPrimitiveType;
use crate::resolve::ItemId;
use crate::types::{TypeId, TypeInfo, TypeTable};

use super::model::LoweringPrepSurfaces;

pub(super) fn substitute_type_id(
    surfaces: &LoweringPrepSurfaces<'_>,
    type_id: TypeId,
    mapping: &HashMap<String, TypeId>,
) -> TypeId {
    match surfaces.types.get(type_id).cloned() {
        Some(TypeInfo::GenericParam(n)) => mapping.get(&n).copied().unwrap_or(type_id),
        Some(TypeInfo::Applied { base, args }) => {
            let new_args: Vec<TypeId> = args.iter().map(|a| substitute_type_id(surfaces, *a, mapping)).collect();
            if new_args == args {
                type_id
            } else {
                find_applied_type(surfaces.types, base, &new_args).unwrap_or(type_id)
            }
        }
        Some(TypeInfo::Array(el)) => {
            let sub = substitute_type_id(surfaces, el, mapping);
            if sub == el { type_id } else { surfaces.types.find_array_of(sub).unwrap_or(type_id) }
        }
        _ => type_id,
    }
}

pub(super) fn primitive_type_id(types: &TypeTable, p: HirPrimitiveType) -> Option<TypeId> {
    types.find_primitive(p)
}

// NOTE: these scan the dense TypeId space and MUST stop at `types.len()`.
// An unbounded `(0..)` iterator never terminates when the target type was never
// interned, because `types.get(id)` returning `None` makes the `find` predicate
// `false` rather than ending iteration (this caused multi-hour CI hangs while
// resolving a named return type that was absent from the lowering surface).
pub(super) fn lookup_function_type(types: &TypeTable, params: &[TypeId], ret: TypeId) -> Option<TypeId> {
    (0..types.len()).map(TypeId).find(|id| matches!(types.get(*id), Some(TypeInfo::Function { params: ps, return_type, }) if ps == params && *return_type == ret))
}

pub(super) fn find_named_type(types: &TypeTable, item: ItemId) -> Option<TypeId> {
    (0..types.len()).map(TypeId).find(|id| matches!(types.get(*id), Some(TypeInfo::Named(i)) if *i == item))
}

pub(super) fn find_generic_param(types: &TypeTable, name: &str) -> Option<TypeId> {
    (0..types.len()).map(TypeId).find(|id| matches!(types.get(*id), Some(TypeInfo::GenericParam(n)) if n == name))
}

pub(super) fn find_applied_type(types: &TypeTable, base: ItemId, args: &[TypeId]) -> Option<TypeId> {
    (0..types.len())
        .map(TypeId)
        .find(|id| matches!(types.get(*id), Some(TypeInfo::Applied { base: b, args: a, }) if *b == base && a == args))
}
