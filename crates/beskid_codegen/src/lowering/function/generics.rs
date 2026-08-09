use std::collections::HashMap;

use beskid_analysis::{
    hir::HirFunctionDefinition,
    resolve::ItemId,
    types::{TypeId, TypeInfo, TypeResult},
};

pub(crate) fn is_self_parameter_function(def: &HirFunctionDefinition) -> bool {
    def.parameters.first().is_some_and(|param| param.node.name.node.name == "self")
}

pub(crate) fn generic_mapping_for_method_receiver(
    type_result: &TypeResult,
    item_id: ItemId,
    receiver_type: TypeId,
) -> HashMap<String, TypeId> {
    let mut mapping = HashMap::new();
    let Some(method_generic_names) = type_result.generic_items.get(&item_id) else {
        return mapping;
    };
    let Some(TypeInfo::Applied { base, args }) = type_result.types.get(receiver_type) else {
        return mapping;
    };
    if method_generic_names.len() == 1 && args.len() == 1 {
        mapping.insert(method_generic_names[0].clone(), args[0]);
        return mapping;
    }
    if let Some(type_generic_names) = type_result.generic_items.get(base) {
        for (name, arg) in type_generic_names.iter().zip(args.iter()) {
            mapping.insert(name.clone(), *arg);
        }
    }
    mapping
}

pub(crate) fn generic_mapping_from_mangled(
    type_result: &TypeResult,
    item_id: ItemId,
    mangled: &str,
) -> Option<HashMap<String, TypeId>> {
    let generic_names = type_result.generic_items.get(&item_id)?;
    let suffix = mangled.rsplit('#').next()?;
    if suffix == mangled {
        return None;
    }
    let type_ids: Vec<TypeId> = suffix.split('_').filter_map(|part| part.parse::<usize>().ok()).map(TypeId).collect();
    if type_ids.len() != generic_names.len() {
        return None;
    }
    Some(generic_names.iter().cloned().zip(type_ids).collect())
}

pub(super) fn substitute_type_id(
    type_result: &TypeResult,
    type_id: beskid_analysis::types::TypeId,
    mapping: &HashMap<String, beskid_analysis::types::TypeId>,
) -> beskid_analysis::types::TypeId {
    let info = type_result.types.get(type_id).cloned();
    match info {
        Some(TypeInfo::GenericParam(name)) => mapping.get(&name).copied().unwrap_or(type_id),
        Some(TypeInfo::Applied { .. }) => type_id,
        _ => type_id,
    }
}
