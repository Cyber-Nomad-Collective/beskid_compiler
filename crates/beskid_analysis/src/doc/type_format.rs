//! HIR type display strings and [`ApiTypeAnnotation`] for `api.json`.

use crate::hir::{HirPrimitiveType, HirType};
use crate::resolve::items::ItemKind;
use crate::resolve::{Resolution, ResolvedType};
use crate::syntax::Spanned;

use super::api_snapshot::ApiTypeAnnotation;
use super::qualified_names::{lookup_type_ref_id, type_ref_lookup_index};

/// Format a HIR type for API documentation display (stable, non-semantic).
pub fn format_hir_type(ty: &Spanned<HirType>) -> String {
    match &ty.node {
        HirType::Primitive(primitive) => match primitive.node {
            HirPrimitiveType::Bool => "bool".to_string(),
            HirPrimitiveType::I32 => "i32".to_string(),
            HirPrimitiveType::I64 => "i64".to_string(),
            HirPrimitiveType::U8 => "u8".to_string(),
            HirPrimitiveType::F64 => "f64".to_string(),
            HirPrimitiveType::Char => "char".to_string(),
            HirPrimitiveType::String => "string".to_string(),
            HirPrimitiveType::Unit => "unit".to_string(),
            HirPrimitiveType::Never => "never".to_string(),
        },
        HirType::Complex(path) => path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.clone())
            .collect::<Vec<_>>()
            .join("."),
        HirType::Array(inner) => format!("{}[]", format_hir_type(inner)),
        HirType::Ref(inner) => format!("ref {}", format_hir_type(inner)),
        HirType::Function {
            return_type,
            parameters,
        } => {
            let params = parameters
                .iter()
                .map(format_hir_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", format_hir_type(return_type), params)
        }
    }
}

fn type_kind_links_to_item(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Type | ItemKind::Enum | ItemKind::Contract
    )
}

/// Build a type annotation with optional cross-link to a resolved type item.
pub fn type_annotation_for_type(
    ty: &Spanned<HirType>,
    resolution: Option<&Resolution>,
) -> ApiTypeAnnotation {
    let display = format_hir_type(ty);
    let ref_item_id = resolution.and_then(|res| {
        let from_span = res.tables.resolved_types.get(&ty.span).and_then(|resolved| {
            match resolved {
                ResolvedType::Item(item_id) => res
                    .items
                    .get(item_id.0)
                    .filter(|item| type_kind_links_to_item(item.kind))
                    .map(|item| item.id.0),
                ResolvedType::Generic(_) => None,
            }
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
