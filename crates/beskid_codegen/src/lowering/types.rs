use beskid_analysis::hir::{HirPrimitiveType, HirType};
use beskid_analysis::resolve::{ItemId, ItemKind, Resolution, ResolvedType};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::{TypeId, TypeInfo, TypeResult};
use cranelift_codegen::ir::types;

use super::expressions::serialize::DYNAMIC_TYPE_NAME;

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
    ty: &Spanned<HirType>,
) -> Option<TypeId> {
    match &ty.node {
        HirType::Primitive(primitive) => find_primitive_type_id(type_result, primitive.node),
        HirType::Complex(_) => match resolution.tables.resolved_types.get(&ty.span)? {
            ResolvedType::Item(item_id) => find_named_type_id(type_result, *item_id),
            ResolvedType::Generic(_) => None,
        },
        HirType::Array(inner) => {
            let inner_id = type_id_for_type(resolution, type_result, inner)?;
            type_result.types.find_array_of(inner_id)
        }
        HirType::Function {
            return_type,
            parameters,
        } => {
            let return_type = type_id_for_type(resolution, type_result, return_type)?;
            let mut params = Vec::with_capacity(parameters.len());
            for parameter in parameters {
                params.push(type_id_for_type(resolution, type_result, parameter)?);
            }
            find_function_type_id(type_result, &params, return_type)
        }
    }
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
    if let Some(type_id) = type_id_for_type(resolution, type_result, def) {
        return Some(type_id);
    }
    if let HirType::Complex(path) = &def.node {
        if let Some(segment) = path.node.segments.last() {
            let name = &segment.node.name.node.name;
            if let Some(item) = resolution
                .items
                .iter()
                .find(|info| info.name == *name && info.kind == ItemKind::Type)
            {
                return find_named_type_id(type_result, item.id);
            }
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
