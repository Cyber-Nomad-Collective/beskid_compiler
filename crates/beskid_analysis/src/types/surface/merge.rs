use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::resolve::ItemId;
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, TypeTable};

use super::model::{MergedTypeEnv, UnitTypeSurface};

pub fn contract_signatures_for_types<'a>(
    target_types: &TypeTable,
    unit_surfaces: impl IntoIterator<Item = &'a UnitTypeSurface>,
) -> HashMap<(ItemId, String), FunctionSignature> {
    let mut types = target_types.clone();
    let mut merged = HashMap::new();
    for surface in unit_surfaces {
        let remap = types.import_from(&surface.types);
        for (key, signature) in &surface.contract_signatures {
            merged.insert(key.clone(), remap_signature(&remap, signature));
        }
    }
    merged
}

/// Merge dependency unit surfaces; `entry_surface` wins on key conflicts.
pub fn merge_unit_surfaces(
    dependency_surfaces: impl Iterator<Item = (PathBuf, Arc<UnitTypeSurface>)>,
    entry_surface: Arc<UnitTypeSurface>,
) -> MergedTypeEnv {
    merge_unit_surfaces_with_types(dependency_surfaces, entry_surface).1
}

/// Merge unit surfaces and their type tables into one canonical [`TypeTable`].
pub fn merge_unit_surfaces_with_types(
    dependency_surfaces: impl Iterator<Item = (PathBuf, Arc<UnitTypeSurface>)>,
    entry_surface: Arc<UnitTypeSurface>,
) -> (TypeTable, MergedTypeEnv) {
    let mut types = TypeTable::new();
    let mut merged = MergedTypeEnv::default();
    for (_, surface) in dependency_surfaces {
        let remap = types.import_from(&surface.types);
        merge_surface_into_remapped(&mut merged, surface.as_ref(), &remap);
    }
    let remap = types.import_from(&entry_surface.types);
    merge_surface_into_remapped(&mut merged, &entry_surface, &remap);
    (types, merged)
}

fn merge_surface_into_remapped(target: &mut MergedTypeEnv, surface: &UnitTypeSurface, remap: &HashMap<TypeId, TypeId>) {
    for (item_id, signature) in &surface.function_signatures {
        target.function_signatures.insert(*item_id, remap_signature(remap, signature));
    }
    for (item_id, signature) in &surface.method_function_signatures {
        target.method_function_signatures.insert(*item_id, remap_signature(remap, signature));
    }
    for (item_id, fields) in &surface.struct_fields_ordered {
        target.struct_fields_ordered.insert(
            *item_id,
            fields.iter().map(|(name, type_id)| (name.clone(), remap_type_id(remap, *type_id))).collect(),
        );
    }
    for (item_id, variants) in &surface.enum_variants_ordered {
        target.enum_variants_ordered.insert(
            *item_id,
            variants
                .iter()
                .map(|(name, fields)| {
                    (name.clone(), fields.iter().map(|type_id| remap_type_id(remap, *type_id)).collect())
                })
                .collect(),
        );
    }
    target.generic_items.extend(surface.generic_items.clone());
    target.struct_event_fields.extend(surface.struct_event_fields.clone());
    for (key, signature) in &surface.contract_signatures {
        target.contract_signatures.insert(key.clone(), remap_signature(remap, signature));
    }
    target.contract_method_order.extend(surface.contract_method_order.clone());
    target.methods_by_receiver.extend(surface.methods_by_receiver.clone());
    target.named_type_names.extend(surface.named_type_names.clone());
}

fn remap_type_id(remap: &HashMap<TypeId, TypeId>, type_id: TypeId) -> TypeId {
    remap.get(&type_id).copied().unwrap_or(type_id)
}

fn remap_signature(remap: &HashMap<TypeId, TypeId>, signature: &FunctionSignature) -> FunctionSignature {
    FunctionSignature {
        params: signature.params.iter().map(|param| remap_type_id(remap, *param)).collect(),
        return_type: remap_type_id(remap, signature.return_type),
    }
}
