//! Syntax type display strings and [`ApiTypeAnnotation`] for `api.json`.

use crate::resolve::items::ItemKind;
use crate::resolve::{Resolution, ResolvedType};
use crate::syntax::{PrimitiveType, Spanned, Type};

use super::api_snapshot::ApiTypeAnnotation;
use super::qualified_names::{lookup_type_ref_id, type_ref_lookup_index};

/// Format a syntax type for API documentation display (stable, non-semantic).
pub fn format_type(ty: &Spanned<Type>) -> String {
    match &ty.node {
        Type::Primitive(primitive) => match primitive.node {
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::I32 => "i32".to_string(),
            PrimitiveType::I64 => "i64".to_string(),
            PrimitiveType::U8 => "u8".to_string(),
            PrimitiveType::Pointer => "pointer".to_string(),
            PrimitiveType::Word => "word".to_string(),
            PrimitiveType::F64 => "f64".to_string(),
            PrimitiveType::Char => "char".to_string(),
            PrimitiveType::String => "string".to_string(),
            PrimitiveType::Unit => "unit".to_string(),
            PrimitiveType::Never => "never".to_string(),
        },
        Type::Complex(path) => path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect::<Vec<_>>()
            .join("."),
        Type::Array(inner) => format!("{}[]", format_type(inner)),
        Type::Function {
            return_type,
            parameters,
        } => {
            let params = parameters
                .iter()
                .map(format_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", format_type(return_type), params)
        }
    }
}

fn type_kind_links_to_item(kind: ItemKind) -> bool {
    matches!(kind, ItemKind::Type | ItemKind::Enum | ItemKind::Contract)
}

/// Build a type annotation with optional cross-link to a resolved type item.
pub fn type_annotation_for_type(
    ty: &Spanned<Type>,
    resolution: Option<&Resolution>,
) -> ApiTypeAnnotation {
    let display = format_type(ty);
    let ref_item_id = resolution.and_then(|res| {
        let from_span =
            res.tables
                .resolved_types
                .get(&ty.span)
                .and_then(|resolved| match resolved {
                    ResolvedType::Item(item_id) => res
                        .items
                        .get(item_id.0)
                        .filter(|item| type_kind_links_to_item(item.kind))
                        .map(|item| item.id.0),
                    ResolvedType::Generic(_) => None,
                });
        if from_span.is_some() {
            return from_span;
        }
        let index = type_ref_lookup_index(res);
        lookup_type_ref_id(&display, &index)
    });
    ApiTypeAnnotation {
        display,
        ref_item_id,
    }
}
