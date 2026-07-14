//! Read contract, event, and name metadata from [`TypeResult`].

use std::collections::HashMap;

use beskid_analysis::resolve::ItemId;
use beskid_analysis::types::FunctionSignature;
use beskid_analysis::types::{
    TypeResult, UnitTypeSurface, contract_signatures_for_types, merge_unit_surfaces,
};

pub(crate) fn named_type_names(type_result: &TypeResult) -> HashMap<ItemId, String> {
    if !type_result.named_type_names.is_empty() {
        return type_result.named_type_names.clone();
    }
    let mut names = HashMap::new();
    for surface in type_result.unit_surfaces.values() {
        names.extend(surface.named_type_names.clone());
    }
    names
}

pub(crate) fn contract_method_order(type_result: &TypeResult) -> HashMap<ItemId, Vec<String>> {
    merge_unit_surfaces(
        type_result
            .unit_surfaces
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
        std::sync::Arc::new(UnitTypeSurface::default()),
    )
    .contract_method_order
}

pub(crate) fn contract_signatures(
    type_result: &TypeResult,
) -> HashMap<(ItemId, String), FunctionSignature> {
    contract_signatures_for_types(
        &type_result.types,
        type_result
            .unit_surfaces
            .values()
            .map(|surface| surface.as_ref()),
    )
}

pub(crate) fn struct_event_fields(
    type_result: &TypeResult,
) -> &HashMap<ItemId, HashMap<String, Option<usize>>> {
    &type_result.struct_event_fields
}
