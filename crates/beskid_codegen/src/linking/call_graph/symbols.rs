use beskid_analysis::resolve::{ItemId, Resolution};
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};

use crate::lowering::function::{mangle_generic_item_function, mangle_method_name};

pub(super) fn symbol_for_call(resolution: &Resolution, item_id: ItemId) -> Option<beskid_analysis::resolve::SymbolId> {
    resolution.items.get(item_id.0).and_then(|info| info.symbol)
}
pub(super) fn build_function_mangled(
    item_id: ItemId,
    generic_args: &[TypeId],
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<String> {
    if generic_args.is_empty() {
        return None;
    }
    let base = resolution.items.get(item_id.0)?.name.clone();
    Some(mangle_generic_item_function(item_id, &base, generic_args, resolution, type_result))
}

pub(super) fn method_mangled_from_receiver(
    method_item_id: ItemId,
    receiver_type: TypeId,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<String> {
    let method_name = resolution.items.get(method_item_id.0)?.name.as_str();
    let receiver_item = match type_result.types.get(receiver_type) {
        Some(TypeInfo::Named(item_id)) => *item_id,
        Some(TypeInfo::Applied { base, .. }) => *base,
        _ => return None,
    };
    let receiver_name = resolution.items.iter().find(|info| info.id == receiver_item).map(|info| info.name.as_str())?;
    Some(mangle_method_name(receiver_name, method_name))
}
