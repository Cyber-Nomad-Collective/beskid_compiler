use beskid_analysis::{
    hir::{HirPrimitiveType, HirType},
    paths::same_file_opt,
    resolve::{ItemId, Resolution},
    syntax::{SpanInfo, Spanned},
    types::{TypeId, TypeInfo, TypeResult},
};
use cranelift_codegen::ir::Signature;

use crate::lowering::{
    type_surface::named_type_names,
    types::{resolve_type_path_item_id_for_codegen, type_id_for_type},
};

pub(super) fn signature_has_return(signature: &Signature) -> bool {
    !signature.returns.is_empty()
}

pub(super) fn resolve_return_type_id(
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&std::path::PathBuf>,
    return_type: Option<&Spanned<HirType>>,
    signature_return: Option<TypeId>,
) -> Option<TypeId> {
    if let Some(annotated) = return_type.and_then(|ty| type_id_for_type(resolution, type_result, source_path, ty)) {
        return Some(annotated);
    }
    return_type.and_then(|ty| fallback_applied_return_type(resolution, type_result, source_path, ty)).or_else(|| {
        signature_return
            .filter(|sig| !matches!(type_result.types.get(*sig), Some(TypeInfo::Primitive(HirPrimitiveType::Unit))))
    })
}

fn fallback_applied_return_type(
    resolution: &Resolution,
    type_result: &TypeResult,
    source_path: Option<&std::path::PathBuf>,
    return_type: &Spanned<HirType>,
) -> Option<TypeId> {
    let beskid_analysis::hir::HirType::Complex(path) = &return_type.node else {
        return None;
    };
    let last = path.node.segments.last()?;
    if last.node.type_args.is_empty() {
        return None;
    }
    let segments: Vec<String> = path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect();
    let mut arg_ids = Vec::with_capacity(last.node.type_args.len());
    for arg in &last.node.type_args {
        let type_id = type_id_for_type(resolution, type_result, source_path, arg).or_else(|| match &arg.node {
            HirType::Primitive(primitive) => find_primitive_type_id(type_result, primitive.node),
            HirType::Complex(path) => {
                let name = path.node.segments.last()?.node.name.node.name.as_str();
                match name {
                    "i64" => find_primitive_type_id(type_result, HirPrimitiveType::I64),
                    "string" => find_primitive_type_id(type_result, HirPrimitiveType::String),
                    _ => find_named_type_by_leaf(type_result, name),
                }
            }
            _ => None,
        })?;
        arg_ids.push(type_id);
    }
    if let Some(base) = resolve_type_path_item_id_for_codegen(resolution, type_result, &segments)
        && let Some(applied) = find_applied_type_id_by_base_and_args(type_result, base, &arg_ids)
    {
        return Some(applied);
    }
    find_applied_type_id_by_args(type_result, &arg_ids)
        .or_else(|| find_applied_type_id_by_ok_arg(type_result, arg_ids.first().copied()))
}

fn find_applied_type_id_by_base_and_args(type_result: &TypeResult, base: ItemId, args: &[TypeId]) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied { base: found_base, args: found_args } = info
            && *found_base == base
            && found_args.as_slice() == args
        {
            return Some(type_id);
        }
        index += 1;
    }
}

fn find_applied_type_id_by_ok_arg(type_result: &TypeResult, ok_type: Option<TypeId>) -> Option<TypeId> {
    let ok_type = ok_type?;
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied { base, args } = info
            && args.first() == Some(&ok_type)
            && args.len() == 2
            && type_result.enum_variants_ordered.contains_key(base)
        {
            return Some(type_id);
        }
        index += 1;
    }
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

fn find_named_type_by_leaf(type_result: &TypeResult, leaf: &str) -> Option<TypeId> {
    for (item_id, name) in &named_type_names(type_result) {
        if name.as_str() == leaf || name.ends_with(&format!("::{leaf}")) {
            let mut index = 0usize;
            loop {
                let type_id = TypeId(index);
                let Some(info) = type_result.types.get(type_id) else {
                    break;
                };
                if matches!(info, TypeInfo::Named(found) if *found == *item_id) {
                    return Some(type_id);
                }
                index += 1;
            }
        }
    }
    None
}

fn find_applied_type_id_by_args(type_result: &TypeResult, args: &[TypeId]) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let type_id = TypeId(index);
        let Some(info) = type_result.types.get(type_id) else {
            return None;
        };
        if let TypeInfo::Applied { args: found_args, .. } = info
            && found_args.as_slice() == args
        {
            return Some(type_id);
        }
        index += 1;
    }
}

pub(crate) fn item_id_for_item_span(
    resolution: &Resolution,
    span: SpanInfo,
    source_path: Option<&std::path::PathBuf>,
) -> Option<ItemId> {
    if let Some(path) = source_path
        && let Some(info) = resolution
            .items
            .iter()
            .find(|info| info.span == span && same_file_opt(info.source_path.as_ref(), Some(path)))
    {
        return Some(info.id);
    }

    let matches: Vec<_> = resolution.items.iter().filter(|info| info.span == span).collect();
    match matches.as_slice() {
        [] => None,
        [single] => Some(single.id),
        _ => None,
    }
}
