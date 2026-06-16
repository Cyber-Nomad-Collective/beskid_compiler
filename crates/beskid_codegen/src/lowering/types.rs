use beskid_analysis::hir::{HirPath, HirPrimitiveType, HirType};
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution, ResolvedType};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};

use crate::linking::resolve_path_item_id;
use crate::lowering::type_surface::named_type_names;
use cranelift_codegen::ir::types;
use std::collections::HashMap;

use super::expressions::serialize::DYNAMIC_TYPE_NAME;

pub(crate) fn resolve_monomorph_type_id(
    type_result: &TypeResult,
    substitution: &HashMap<String, TypeId>,
    type_id: TypeId,
) -> TypeId {
    match type_result.types.get(type_id) {
        Some(TypeInfo::GenericParam(name)) => substitution.get(name).copied().unwrap_or(type_id),
        _ => type_id,
    }
}

pub(crate) fn is_fiber_handle_type(
    type_result: &TypeResult,
    resolution: &Resolution,
    type_id: TypeId,
) -> bool {
    match type_result.types.get(type_id) {
        Some(TypeInfo::Fiber(_)) => true,
        Some(TypeInfo::Applied { base, .. }) | Some(TypeInfo::Named(base)) => resolution
            .items
            .get(base.0)
            .is_some_and(|info| info.name == "Fiber" || info.name.ends_with("::Fiber")),
        _ => false,
    }
}

pub(crate) fn map_type_id_to_clif(
    type_result: &TypeResult,
    type_id: TypeId,
) -> Option<cranelift_codegen::ir::Type> {
    match type_result.types.get(type_id) {
        Some(TypeInfo::Primitive(primitive)) => map_primitive_to_clif(*primitive),
        Some(TypeInfo::Array(_)) | Some(TypeInfo::Fiber(_)) => Some(pointer_type()),
        Some(TypeInfo::Named(_))
        | Some(TypeInfo::GenericParam(_))
        | Some(TypeInfo::Applied { .. })
        | Some(TypeInfo::Function { .. }) => Some(pointer_type()),
        _ => None,
    }
}

/// Whether `type_id` resolves to the v0.3 `dynamic` cell type (named alias until primitive lands).
pub fn is_dynamic_type_id(
    resolution: &Resolution,
    type_result: &TypeResult,
    type_id: TypeId,
) -> bool {
    match type_result.types.get(type_id) {
        Some(TypeInfo::Named(item_id)) => resolution
            .items
            .iter()
            .any(|item| item.id == *item_id && item.name == DYNAMIC_TYPE_NAME),
        _ => false,
    }
}

/// CLIF representation for `dynamic`: pointer to [`beskid_runtime::dynamic::DynamicCell`].
pub fn dynamic_clif_type() -> cranelift_codegen::ir::Type {
    pointer_type()
}

/// Map a type to CLIF, treating the `dynamic` named alias as a cell pointer.
pub fn map_type_id_to_clif_with_dynamic(
    resolution: &Resolution,
    type_result: &TypeResult,
    type_id: TypeId,
) -> Option<cranelift_codegen::ir::Type> {
    if is_dynamic_type_id(resolution, type_result, type_id) {
        return Some(dynamic_clif_type());
    }
    map_type_id_to_clif(type_result, type_id)
}

fn find_function_type_id(
    type_result: &TypeResult,
    params: &[TypeId],
    return_type: TypeId,
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Function {
            params: candidate_params,
            return_type: candidate_return,
        } = info
            && candidate_return == &return_type
            && candidate_params.as_slice() == params
        {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn type_id_for_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&std::path::PathBuf>,
    ty: &Spanned<HirType>,
) -> Option<TypeId> {
    match &ty.node {
        HirType::Primitive(primitive) => find_primitive_type_id(type_result, primitive.node),
        HirType::Complex(path) => type_id_for_complex_type(resolution, type_result, source_path, path),
        HirType::Array(inner) => {
            let inner_id = type_id_for_type(resolution, type_result, source_path, inner)?;
            type_result.types.find_array_of(inner_id)
        }
        HirType::Function {
            return_type,
            parameters,
        } => {
            let return_type = type_id_for_type(resolution, type_result, source_path, return_type)?;
            let mut params = Vec::with_capacity(parameters.len());
            for parameter in parameters {
                params.push(type_id_for_type(
                    resolution,
                    type_result,
                    source_path,
                    parameter,
                )?);
            }
            find_function_type_id(type_result, &params, return_type)
        }
    }
}

fn primitive_type_id_for_name(type_result: &TypeResult, name: &str) -> Option<TypeId> {
    let primitive = match name {
        "bool" => HirPrimitiveType::Bool,
        "i32" => HirPrimitiveType::I32,
        "i64" => HirPrimitiveType::I64,
        "u8" => HirPrimitiveType::U8,
        "f64" => HirPrimitiveType::F64,
        "char" => HirPrimitiveType::Char,
        "string" => HirPrimitiveType::String,
        _ => return None,
    };
    find_primitive_type_id(type_result, primitive)
}

fn primitive_type_id_for_item(
    resolution: &Resolution,
    type_result: &TypeResult,
    item_id: beskid_analysis::resolve::ItemId,
) -> Option<TypeId> {
    let name = resolution.items.get(item_id.0)?.name.as_str();
    let leaf = name.rsplit("::").next().unwrap_or(name);
    primitive_type_id_for_name(type_result, leaf)
}

pub fn pointer_type() -> cranelift_codegen::ir::Type {
    types::I64
}

fn find_primitive_type_id(type_result: &TypeResult, primitive: HirPrimitiveType) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Primitive(found) if *found == primitive) {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn map_primitive_to_clif(
    primitive: HirPrimitiveType,
) -> Option<cranelift_codegen::ir::Type> {
    match primitive {
        HirPrimitiveType::Bool => Some(types::I8),
        HirPrimitiveType::I32 => Some(types::I32),
        HirPrimitiveType::I64 => Some(types::I64),
        HirPrimitiveType::U8 => Some(types::I8),
        HirPrimitiveType::F64 => Some(types::F64),
        HirPrimitiveType::Unit => None,
        HirPrimitiveType::Never => None,
        HirPrimitiveType::Char => Some(types::I32),
        HirPrimitiveType::String => Some(pointer_type()),
    }
}

fn type_id_for_complex_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&std::path::PathBuf>,
    path: &Spanned<HirPath>,
) -> Option<TypeId> {
    if let Some(last_segment) = path.node.segments.last()
        && !last_segment.node.type_args.is_empty()
    {
        let segments: Vec<String> = path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect();
        let mut args = Vec::with_capacity(last_segment.node.type_args.len());
        for arg in &last_segment.node.type_args {
            args.push(type_id_for_type(resolution, type_result, source_path, arg)?);
        }
        if let Some(base) = resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
            && let Some(applied) = find_applied_type_id(type_result, base, &args)
        {
            return Some(applied);
        }
        return find_applied_type_id_by_args(type_result, &args);
    }

    if let Some(resolved) = resolution.tables.resolved_type_at(path.span, source_path) {
        match resolved {
            ResolvedType::Item(item_id) => find_named_type_id(type_result, item_id)
                .or_else(|| find_applied_type_id_for_base(type_result, item_id))
                .or_else(|| primitive_type_id_for_item(resolution, type_result, item_id)),
            ResolvedType::Generic(_) => None,
        }
    } else {
        let segments: Vec<String> = path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect();
        resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
            .and_then(|item_id| {
                find_named_type_id(type_result, item_id)
                    .or_else(|| find_applied_type_id_for_base(type_result, item_id))
            })
            .or_else(|| {
                if path.node.segments.len() == 1 && path.node.segments[0].node.type_args.is_empty() {
                    let name = path.node.segments[0].node.name.node.name.as_str();
                    primitive_type_id_for_name(type_result, name)
                } else {
                    None
                }
            })
    }
}

fn find_applied_type_id_by_args(type_result: &TypeResult, args: &[TypeId]) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied {
            args: found_args, ..
        } = info
            && found_args.as_slice() == args
        {
            return Some(type_id);
        }
        index += 1;
    }
}

fn find_applied_type_id(
    type_result: &TypeResult,
    base: ItemId,
    args: &[TypeId],
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied {
            base: found_base,
            args: found_args,
        } = info
            && *found_base == base
            && found_args.as_slice() == args
        {
            return Some(type_id);
        }
        index += 1;
    }
}

fn find_applied_type_id_for_base(type_result: &TypeResult, base: ItemId) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Applied { base: found, .. } if *found == base) {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn resolve_type_path_item_id_for_codegen(
    resolution: &Resolution,
    type_result: &TypeResult,
    segments: &[String],
) -> Option<ItemId> {
    if let Some(item_id) = resolve_path_item_id(resolution, segments) {
        return Some(item_id);
    }
    if segments.len() >= 2 {
        let name = segments.last()?;
        if let Some(module_id) = resolution.module_graph.module_id(segments)
            && let Some(module) = resolution.module_graph.module(module_id)
            && let Some(item_id) = module.scope.get(name)
        {
            return Some(*item_id);
        }
    }
    let name = segments.last()?;
    for (item_id, type_name) in &named_type_names(type_result) {
        if (type_name.as_str() == name.as_str() || type_name.ends_with(&format!("::{name}")))
            && (type_result.enum_variants_ordered.contains_key(item_id)
                || type_result.struct_fields_ordered.contains_key(item_id))
        {
            return Some(*item_id);
        }
    }
    resolution
        .items
        .iter()
        .find(|info| {
            matches!(info.kind, ItemKind::Type | ItemKind::Enum)
                && (info.name.as_str() == name.as_str()
                    || info.name.ends_with(&format!("::{name}")))
        })
        .map(|info| info.id)
}

fn find_named_type_id(
    type_result: &TypeResult,
    item_id: beskid_analysis::resolve::ItemId,
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if matches!(info, TypeInfo::Named(found) if *found == item_id) {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn method_receiver_type_id(
    resolution: &Resolution,
    type_result: &TypeResult,
    def: &Spanned<HirType>,
    method_item_id: ItemId,
) -> Option<TypeId> {
    if let Some(type_id) = type_id_for_type(resolution, type_result, None, def) {
        return Some(type_id);
    }
    if let HirType::Complex(path) = &def.node
        && let Some(segment) = path.node.segments.last() {
            let name = &segment.node.name.node.name;
            if let Some(item) = resolution
                .items
                .iter()
                .find(|info| info.name == *name && info.kind == ItemKind::Type)
            {
                return find_named_type_id(type_result, item.id);
            }
        }
    let info = resolution.items.get(method_item_id.0)?;
    let (receiver_name, _) = info.name.split_once("::")?;
    let receiver_item = resolution
        .items
        .iter()
        .find(|item| item.name == receiver_name && item.kind == ItemKind::Type)?;
    find_named_type_id(type_result, receiver_item.id)
}
