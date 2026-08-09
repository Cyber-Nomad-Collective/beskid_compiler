use beskid_analysis::{
    hir::HirFunctionDefinition,
    paths::same_file_opt,
    resolve::{ItemId, Resolution},
    syntax::Spanned,
    types::{TypeId, TypeInfo, TypeResult},
};

use crate::lowering::expressions::export::{export_linker_name, read_export_metadata};

pub(crate) fn mangle_method_name(receiver: &str, method: &str) -> String {
    let receiver_short = receiver.rsplit("::").next().unwrap_or(receiver);
    let method_short = method.rsplit("::").next().unwrap_or(method);
    format!("__method__{receiver_short}__{method_short}")
}

pub(crate) fn mangle_function_name(base: &str, args: &[beskid_analysis::types::TypeId]) -> String {
    if args.is_empty() {
        return base.to_string();
    }
    let suffix = args.iter().map(|arg| arg.0.to_string()).collect::<Vec<_>>().join("_");
    format!("{base}#{suffix}")
}

/// Disambiguate non-generic link-plan functions that share a short name across modules (`Contains#42`).
pub(crate) fn mangle_item_function(resolution: &Resolution, item_id: ItemId) -> String {
    let info = resolution.items.get(item_id.0).unwrap_or_else(|| panic!("missing item for mangling: {:?}", item_id));
    let short = info.name.rsplit("::").next().unwrap_or(info.name.as_str());
    format!("{short}#{}", item_id.0)
}

pub(crate) fn linker_name_for_item_function(
    resolution: &Resolution,
    item_id: ItemId,
    def: &Spanned<HirFunctionDefinition>,
) -> String {
    if read_export_metadata(def).is_some() {
        export_linker_name(def)
    } else {
        mangle_item_function(resolution, item_id)
    }
}

/// Stem-qualified mangling for generic factory functions on owning types (`Hub__Create#2`).
pub(crate) fn mangle_generic_factory_name(owner_stem: &str, method: &str, args: &[TypeId]) -> String {
    let leaf = method.rsplit("::").next().unwrap_or(method);
    mangle_function_name(&format!("{owner_stem}__{leaf}"), args)
}

/// When a generic function returns `Owner<T>` from the same source file as `Owner`, qualify the symbol stem.
pub(crate) fn owner_stem_for_generic_factory(
    item_id: ItemId,
    resolution: &Resolution,
    type_result: &TypeResult,
) -> Option<String> {
    let generic_names = type_result.generic_items.get(&item_id)?;
    if generic_names.is_empty() {
        return None;
    }
    let sig = type_result.function_signatures.get(&item_id)?;
    let TypeInfo::Applied { base, .. } = type_result.types.get(sig.return_type)? else {
        return None;
    };
    let func_info = resolution.items.get(item_id.0)?;
    let owner_info = resolution.items.iter().find(|info| info.id == *base)?;
    if !same_file_opt(func_info.source_path.as_ref(), owner_info.source_path.as_ref()) {
        return None;
    }
    owner_info.name.rsplit("::").next().map(str::to_string)
}

pub(crate) fn mangle_generic_item_function(
    item_id: ItemId,
    base: &str,
    generic_args: &[TypeId],
    resolution: &Resolution,
    type_result: &TypeResult,
) -> String {
    let leaf = base.rsplit("::").next().unwrap_or(base);
    if !generic_args.is_empty()
        && let Some(stem) = owner_stem_for_generic_factory(item_id, resolution, type_result)
    {
        return mangle_generic_factory_name(&stem, leaf, generic_args);
    }
    mangle_function_name(leaf, generic_args)
}
