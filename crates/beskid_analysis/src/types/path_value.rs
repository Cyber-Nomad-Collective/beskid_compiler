//! Shared read-only helpers for value-path local resolution and struct field chains.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::hir::HirPath;
use crate::paths;
use crate::resolve::{ItemId, ItemKind, LocalId, Resolution, ResolvedValue};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::context::context::TypeResult;
use crate::types::{TypeId, TypeInfo, TypeTable};

/// Read-only type environment for path field lookup (shared by [`TypeResult`] and [`TypeContext`](crate::types::context::TypeContext)).
pub struct PathTypeEnv<'a> {
    pub types: &'a TypeTable,
    pub local_types: &'a HashMap<LocalId, TypeId>,
    pub struct_fields_ordered: &'a HashMap<ItemId, Vec<(String, TypeId)>>,
    pub generic_items: &'a HashMap<ItemId, Vec<String>>,
}

impl TypeResult {
    pub fn path_env(&self) -> PathTypeEnv<'_> {
        PathTypeEnv {
            types: &self.types,
            local_types: &self.local_types,
            struct_fields_ordered: &self.struct_fields_ordered,
            generic_items: &self.generic_items,
        }
    }
}

/// Resolve the base [`LocalId`] for a multi-segment value path span.
pub fn resolve_path_base_local(
    resolution: &Resolution,
    path_span: SpanInfo,
    first_segment: &str,
    source_path: Option<&PathBuf>,
) -> Option<LocalId> {
    if let Some(ResolvedValue::Local(local_id)) =
        resolution.tables.resolved_value_at(path_span, source_path)
    {
        return Some(local_id);
    }

    if let Some(local_id) = resolution.tables.local_id_for_span(path_span, source_path) {
        return Some(local_id);
    }

    let source = source_path?;
    let scoped: Vec<_> = resolution
        .tables
        .locals
        .iter()
        .filter(|info| {
            info.name == first_segment
                && info
                    .source_path
                    .as_ref()
                    .is_some_and(|local_path| paths::same_file(local_path, source))
        })
        .collect();
    match scoped.as_slice() {
        [single] => Some(single.id),
        _ => None,
    }
}

pub fn named_item_id(env: &PathTypeEnv<'_>, type_id: TypeId) -> Option<ItemId> {
    match env.types.get(type_id) {
        Some(TypeInfo::Named(item_id)) => Some(*item_id),
        Some(TypeInfo::Applied { base, .. }) => Some(*base),
        _ => None,
    }
}

pub fn generic_mapping_for_type_id(
    env: &PathTypeEnv<'_>,
    type_id: TypeId,
) -> HashMap<String, TypeId> {
    let Some(TypeInfo::Applied { base, args }) = env.types.get(type_id) else {
        return HashMap::new();
    };
    let Some(names) = env.generic_items.get(base) else {
        return HashMap::new();
    };
    if names.len() != args.len() {
        return HashMap::new();
    }
    names.iter().cloned().zip(args.iter().copied()).collect()
}

fn find_matching_type_id(
    env: &PathTypeEnv<'_>,
    matches: impl Fn(&TypeInfo) -> bool,
) -> Option<TypeId> {
    let mut index = 0usize;
    loop {
        let candidate = TypeId(index);
        let Some(info) = env.types.get(candidate) else {
            break;
        };
        if matches(info) {
            return Some(candidate);
        }
        index += 1;
    }
    None
}

fn substitute_type_id_readonly(
    env: &PathTypeEnv<'_>,
    type_id: TypeId,
    mapping: &HashMap<String, TypeId>,
) -> TypeId {
    if mapping.is_empty() {
        return type_id;
    }
    match env.types.get(type_id) {
        Some(TypeInfo::GenericParam(name)) => mapping.get(name).copied().unwrap_or(type_id),
        Some(TypeInfo::Applied { base, args }) => {
            let new_args: Vec<TypeId> = args
                .iter()
                .map(|arg| substitute_type_id_readonly(env, *arg, mapping))
                .collect();
            find_matching_type_id(env, |info| {
                matches!(
                    info,
                    TypeInfo::Applied {
                        base: existing_base,
                        args: existing_args,
                    } if *existing_base == *base && existing_args == &new_args
                )
            })
            .unwrap_or(type_id)
        }
        Some(TypeInfo::Array(element)) => {
            let substituted = substitute_type_id_readonly(env, *element, mapping);
            if substituted == *element {
                return type_id;
            }
            find_matching_type_id(
                env,
                |info| matches!(info, TypeInfo::Array(existing) if *existing == substituted),
            )
            .unwrap_or(type_id)
        }
        _ => type_id,
    }
}

fn item_id_for_name(
    resolution: &Resolution,
    name: &str,
    kind: ItemKind,
    source_path: Option<&PathBuf>,
) -> Option<ItemId> {
    let matches: Vec<_> = resolution
        .items
        .iter()
        .filter(|info| info.name == name && info.kind == kind)
        .collect();
    match matches.as_slice() {
        [] => None,
        [single] => Some(single.id),
        many => {
            if let Some(path) = source_path
                && let Some(info) = many.iter().rev().find(|info| {
                    info.source_path
                        .as_ref()
                        .is_some_and(|source| paths::same_file(source, path))
                }) {
                    return Some(info.id);
                }
            many.last().map(|info| info.id)
        }
    }
}

pub fn struct_fields_for_item<'a>(
    env: &'a PathTypeEnv<'_>,
    resolution: &Resolution,
    item_id: ItemId,
    source_path: Option<&PathBuf>,
) -> Option<&'a Vec<(String, TypeId)>> {
    if let Some(fields) = env.struct_fields_ordered.get(&item_id) {
        return Some(fields);
    }
    resolution
        .items
        .iter()
        .find(|info| info.id == item_id)
        .and_then(|info| item_id_for_name(resolution, &info.name, ItemKind::Type, source_path))
        .and_then(|item_id| env.struct_fields_ordered.get(&item_id))
}

/// Look up a struct field on `receiver_type`, applying generic substitution when needed.
pub fn field_type_on_receiver(
    resolution: &Resolution,
    env: &PathTypeEnv<'_>,
    receiver_type: TypeId,
    field_name: &str,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    let item_id = named_item_id(env, receiver_type)?;
    let fields = struct_fields_for_item(env, resolution, item_id, source_path)?;
    let field_type = fields
        .iter()
        .find(|(name, _)| name.as_str() == field_name)
        .map(|(_, field_type)| *field_type)?;
    let mapping = generic_mapping_for_type_id(env, receiver_type);
    Some(substitute_type_id_readonly(env, field_type, &mapping))
}

/// Infer the type of a multi-segment value path rooted at a local (`local.a.b`).
pub fn field_type_for_value_path(
    resolution: &Resolution,
    env: &PathTypeEnv<'_>,
    path_span: SpanInfo,
    path: &Spanned<HirPath>,
    source_path: Option<&PathBuf>,
) -> Option<TypeId> {
    let segments = &path.node.segments;
    if segments.len() < 2 {
        return None;
    }
    let first_name = segments.first()?.node.name.node.name.as_str();
    let local_id = resolve_path_base_local(resolution, path_span, first_name, source_path)?;
    let mut current_type = env.local_types.get(&local_id).copied()?;
    for segment in segments.iter().skip(1) {
        current_type = field_type_on_receiver(
            resolution,
            env,
            current_type,
            segment.node.name.node.name.as_str(),
            source_path,
        )?;
    }
    Some(current_type)
}

/// Field segment names between the base local and a trailing method name (`a.b.method` → `["b"]`).
pub fn field_segments_before_method(
    segments: &[Spanned<crate::hir::HirPathSegment>],
) -> &[Spanned<crate::hir::HirPathSegment>] {
    if segments.len() <= 2 {
        &[]
    } else {
        &segments[1..segments.len() - 1]
    }
}

/// Method name for a dotted path callee (`local.method` or `local.field.method`).
pub fn method_name_from_path_callee(
    segments: &[Spanned<crate::hir::HirPathSegment>],
) -> Option<&str> {
    segments
        .last()
        .map(|segment| segment.node.name.node.name.as_str())
}

/// Receiver type after walking field segments before the method name on a path callee.
pub fn receiver_type_for_path_callee(
    resolution: &Resolution,
    env: &PathTypeEnv<'_>,
    path_span: SpanInfo,
    segments: &[Spanned<crate::hir::HirPathSegment>],
    source_path: Option<&PathBuf>,
) -> Option<(LocalId, TypeId)> {
    if segments.len() < 2 {
        return None;
    }
    let first_name = segments.first()?.node.name.node.name.as_str();
    let local_id = resolve_path_base_local(resolution, path_span, first_name, source_path)?;
    let mut receiver_type = env.local_types.get(&local_id).copied()?;
    for segment in field_segments_before_method(segments) {
        receiver_type = field_type_on_receiver(
            resolution,
            env,
            receiver_type,
            segment.node.name.node.name.as_str(),
            source_path,
        )?;
    }
    Some((local_id, receiver_type))
}

/// First field segment on a path rooted at a local (`local.eventField` → `"eventField"`).
pub fn first_field_segment_name(segments: &[Spanned<crate::hir::HirPathSegment>]) -> Option<&str> {
    segments
        .get(1)
        .map(|segment| segment.node.name.node.name.as_str())
}
