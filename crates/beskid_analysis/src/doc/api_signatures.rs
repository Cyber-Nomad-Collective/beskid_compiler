//! Compiler-derived signatures and type annotations for `api.json` rows.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::hir::{
    AstProgram, HirContractMethodSignature, HirEnumDefinition, HirEnumVariant, HirField,
    HirFunctionDefinition, HirItem, HirMethodDefinition, HirParameter, HirProgram,
    HirTypeDefinition, lower_program as lower_hir_program, normalize_program,
};
use crate::projects::assembly::SourceUnit;
use crate::resolve::Resolution;
use crate::resolve::items::{ItemInfo, ItemKind};
use crate::syntax::{SpanInfo, Spanned};

use super::api_snapshot::{
    ApiGenericParameterDoc, ApiItemSignature, ApiParameterDoc, ApiTypeAnnotation,
};
use super::qualified_names::{display_name_for_item, module_path_for_item};
use super::type_format::type_annotation_for_type;

enum HirDeclAtSpan<'a> {
    Function(&'a Spanned<HirFunctionDefinition>),
    Method(&'a Spanned<HirMethodDefinition>),
    Type(&'a Spanned<HirTypeDefinition>),
    Enum(&'a Spanned<HirEnumDefinition>),
    EnumVariant(&'a Spanned<HirEnumVariant>),
    Field(&'a Spanned<HirField>),
    Parameter(&'a Spanned<HirParameter>),
    ContractMethod(&'a Spanned<HirContractMethodSignature>),
}

fn spans_equal(a: SpanInfo, b: SpanInfo) -> bool {
    a.start == b.start && a.end == b.end
}

fn find_decl_at_span<'a>(
    program: &'a Spanned<HirProgram>,
    span: SpanInfo,
) -> Option<HirDeclAtSpan<'a>> {
    for item in &program.node.items {
        if let Some(found) = find_decl_in_item(item, span) {
            return Some(found);
        }
    }
    None
}

fn find_decl_in_item<'a>(item: &'a Spanned<HirItem>, span: SpanInfo) -> Option<HirDeclAtSpan<'a>> {
    if spans_equal(item.span, span) {
        return match &item.node {
            HirItem::FunctionDefinition(def) => Some(HirDeclAtSpan::Function(def)),
            HirItem::MethodDefinition(def) => Some(HirDeclAtSpan::Method(def)),
            HirItem::TypeDefinition(def) => Some(HirDeclAtSpan::Type(def)),
            HirItem::EnumDefinition(def) => Some(HirDeclAtSpan::Enum(def)),
            HirItem::ContractDefinition(def) => {
                for node in &def.node.items {
                    if let crate::hir::HirContractNode::MethodSignature(sig) = &node.node
                        && spans_equal(node.span, span)
                    {
                        return Some(HirDeclAtSpan::ContractMethod(sig));
                    }
                }
                None
            }
            _ => None,
        };
    }
    match &item.node {
        HirItem::FunctionDefinition(def) => find_parameter(def.node.parameters.iter(), span),
        HirItem::MethodDefinition(def) => find_parameter(def.node.parameters.iter(), span),
        HirItem::TypeDefinition(def) => find_field(def.node.fields.iter(), span),
        HirItem::EnumDefinition(def) => {
            for variant in &def.node.variants {
                if spans_equal(variant.span, span) {
                    return Some(HirDeclAtSpan::EnumVariant(variant));
                }
                if let Some(f) = find_field(variant.node.fields.iter(), span) {
                    return Some(f);
                }
            }
            None
        }
        HirItem::ExtendTypeDefinition(def) => {
            for method in &def.node.methods {
                if let Some(p) = find_parameter(method.node.parameters.iter(), span) {
                    return Some(p);
                }
                if spans_equal(method.span, span) {
                    return Some(HirDeclAtSpan::Method(method));
                }
            }
            None
        }
        HirItem::InlineModule(im) => {
            for nested in &im.node.items {
                if let Some(found) = find_decl_in_item(nested, span) {
                    return Some(found);
                }
            }
            None
        }
        HirItem::ContractDefinition(def) => {
            for node in &def.node.items {
                if let crate::hir::HirContractNode::MethodSignature(sig) = &node.node {
                    if spans_equal(node.span, span) {
                        return Some(HirDeclAtSpan::ContractMethod(sig));
                    }
                    if let Some(p) = find_parameter(sig.node.parameters.iter(), span) {
                        return Some(p);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn find_parameter<'a>(
    mut parameters: impl Iterator<Item = &'a Spanned<HirParameter>>,
    span: SpanInfo,
) -> Option<HirDeclAtSpan<'a>> {
    parameters
        .find(|p| spans_equal(p.span, span))
        .map(HirDeclAtSpan::Parameter)
}

fn find_field<'a>(
    mut fields: impl Iterator<Item = &'a Spanned<HirField>>,
    span: SpanInfo,
) -> Option<HirDeclAtSpan<'a>> {
    fields
        .find(|f| spans_equal(f.span, span))
        .map(HirDeclAtSpan::Field)
}

fn parameter_modifier(parameter: &HirParameter) -> Option<String> {
    parameter.mutable.then(|| "mut".to_string())
}

fn generic_parameters_from_names(names: &[String]) -> Vec<ApiGenericParameterDoc> {
    names
        .iter()
        .map(|name| ApiGenericParameterDoc { name: name.clone() })
        .collect()
}

fn callable_parameters(
    parameters: &[Spanned<HirParameter>],
    resolution: Option<&Resolution>,
) -> Vec<ApiParameterDoc> {
    parameters
        .iter()
        .map(|parameter| ApiParameterDoc {
            name: parameter.node.name.node.name.clone(),
            ty: type_annotation_for_type(&parameter.node.ty, resolution),
            modifier: parameter_modifier(&parameter.node),
            doc_markdown: None,
        })
        .collect()
}

fn return_annotation(
    return_type: Option<&Spanned<crate::hir::HirType>>,
    resolution: Option<&Resolution>,
) -> Option<ApiTypeAnnotation> {
    return_type.map(|ty| type_annotation_for_type(ty, resolution))
}

fn build_from_decl(
    decl: HirDeclAtSpan<'_>,
    item: &ItemInfo,
    resolution: Option<&Resolution>,
) -> ApiItemSignature {
    let mut sig = ApiItemSignature {
        display_name: Some(display_name_for_item(item)),
        module_path: resolution
            .map(|r| module_path_for_item(item, &r.module_graph))
            .unwrap_or_default(),
        ..Default::default()
    };

    match decl {
        HirDeclAtSpan::Function(def) => {
            let return_ty = return_annotation(def.node.return_type.as_ref(), resolution);
            let params = callable_parameters(&def.node.parameters, resolution);
            let ret_display = return_ty
                .as_ref()
                .map(|t| t.display.clone())
                .unwrap_or_else(|| "unit".to_string());
            let param_display = params
                .iter()
                .map(|p| {
                    let mut_prefix = p
                        .modifier
                        .as_ref()
                        .map(|m| format!("{m} "))
                        .unwrap_or_default();
                    format!("{mut_prefix}{} {}", p.ty.display, p.name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            sig.return_type = return_ty;
            sig.parameters = params;
            sig.generic_parameters = generic_parameters_from_names(
                &def.node
                    .generics
                    .iter()
                    .map(|g| g.node.name.clone())
                    .collect::<Vec<_>>(),
            );
            sig.signature = Some(format!(
                "{} {}({})",
                ret_display, def.node.name.node.name, param_display
            ));
        }
        HirDeclAtSpan::Method(def) => {
            let return_ty = return_annotation(def.node.return_type.as_ref(), resolution);
            let params = callable_parameters(&def.node.parameters, resolution);
            let ret_display = return_ty
                .as_ref()
                .map(|t| t.display.clone())
                .unwrap_or_else(|| "unit".to_string());
            let param_display = params
                .iter()
                .map(|p| {
                    let mut_prefix = p
                        .modifier
                        .as_ref()
                        .map(|m| format!("{m} "))
                        .unwrap_or_default();
                    format!("{mut_prefix}{} {}", p.ty.display, p.name)
                })
                .collect::<Vec<_>>()
                .join(", ");
            sig.return_type = return_ty;
            sig.parameters = params;
            sig.signature = Some(format!(
                "{} {}({})",
                ret_display, def.node.name.node.name, param_display
            ));
        }
        HirDeclAtSpan::Type(def) => {
            let generics: Vec<_> = def
                .node
                .generics
                .iter()
                .map(|g| g.node.name.clone())
                .collect();
            sig.generic_parameters = generic_parameters_from_names(&generics);
            let g = if generics.is_empty() {
                String::new()
            } else {
                format!("<{}>", generics.join(", "))
            };
            sig.signature = Some(format!("type {}{}", def.node.name.node.name, g));
        }
        HirDeclAtSpan::Enum(def) => {
            let generics: Vec<_> = def
                .node
                .generics
                .iter()
                .map(|g| g.node.name.clone())
                .collect();
            sig.generic_parameters = generic_parameters_from_names(&generics);
            let g = if generics.is_empty() {
                String::new()
            } else {
                format!("<{}>", generics.join(", "))
            };
            sig.signature = Some(format!("enum {}{}", def.node.name.node.name, g));
        }
        HirDeclAtSpan::EnumVariant(variant) => {
            sig.signature = Some(format!("enum variant {}", variant.node.name.node.name));
        }
        HirDeclAtSpan::Field(field) => {
            let field_type = type_annotation_for_type(&field.node.ty, resolution);
            let display = field_type.display.clone();
            sig.field_type = Some(field_type);
            sig.signature = Some(format!("{} {}", display, field.node.name.node.name));
        }
        HirDeclAtSpan::Parameter(parameter) => {
            let field_type = type_annotation_for_type(&parameter.node.ty, resolution);
            let display = field_type.display.clone();
            sig.field_type = Some(field_type);
            sig.signature = Some(format!("{} {}", display, parameter.node.name.node.name));
        }
        HirDeclAtSpan::ContractMethod(method) => {
            let return_ty = return_annotation(method.node.return_type.as_ref(), resolution);
            let params = callable_parameters(&method.node.parameters, resolution);
            let ret_display = return_ty
                .as_ref()
                .map(|t| t.display.clone())
                .unwrap_or_else(|| "unit".to_string());
            let param_display = params
                .iter()
                .map(|p| format!("{} {}", p.ty.display, p.name))
                .collect::<Vec<_>>()
                .join(", ");
            sig.return_type = return_ty;
            sig.parameters = params;
            sig.signature = Some(format!(
                "{} {}({})",
                ret_display, method.node.name.node.name, param_display
            ));
        }
    }
    sig
}

fn lower_unit_hir(unit: &SourceUnit) -> Option<Spanned<HirProgram>> {
    let ast: Spanned<AstProgram> = unit.program.clone().into();
    let mut hir = lower_hir_program(&ast);
    normalize_program(&mut hir).ok()?;
    Some(hir)
}

/// Lowered HIR per compilation unit path (for assembly-backed doc runs).
pub fn hir_programs_by_path(units: &[SourceUnit]) -> HashMap<PathBuf, Spanned<HirProgram>> {
    let mut map = HashMap::new();
    for unit in units {
        if let Some(hir) = lower_unit_hir(unit) {
            map.insert(unit.path.clone(), hir);
        }
    }
    map
}

fn hir_for_item<'a>(
    item: &ItemInfo,
    hir_by_path: &'a HashMap<PathBuf, Spanned<HirProgram>>,
    fallback: Option<&'a Spanned<HirProgram>>,
) -> Option<&'a Spanned<HirProgram>> {
    if let Some(path) = item.source_path.as_ref()
        && let Some(hir) = hir_by_path.get(path)
    {
        return Some(hir);
    }
    fallback
}

/// Build compiler-derived signature fields for one resolved item.
pub fn build_item_signature(
    item: &ItemInfo,
    resolution: Option<&Resolution>,
    hir_by_path: &HashMap<PathBuf, Spanned<HirProgram>>,
    fallback_hir: Option<&Spanned<HirProgram>>,
) -> ApiItemSignature {
    let mut base = ApiItemSignature {
        display_name: Some(display_name_for_item(item)),
        module_path: resolution
            .map(|r| module_path_for_item(item, &r.module_graph))
            .unwrap_or_default(),
        ..Default::default()
    };

    let Some(hir) = hir_for_item(item, hir_by_path, fallback_hir) else {
        if matches!(item.kind, ItemKind::Statement) {
            base.signature = Some("statement".to_string());
        }
        return base;
    };

    let Some(decl) = find_decl_at_span(hir, item.span) else {
        return base;
    };

    let mut built = build_from_decl(decl, item, resolution);
    if built.display_name.is_none() {
        built.display_name = base.display_name;
    }
    if built.module_path.is_empty() {
        built.module_path = base.module_path;
    }
    built
}

/// Apply [`ApiItemSignature`] fields onto an [`super::api_snapshot::ApiDocItem`].
pub fn apply_signature_to_item(item: &mut super::api_snapshot::ApiDocItem, sig: ApiItemSignature) {
    item.display_name = sig.display_name;
    item.module_path = sig.module_path;
    item.signature = sig.signature;
    item.field_type = sig.field_type;
    item.return_type = sig.return_type;
    item.parameters = sig.parameters;
    item.generic_parameters = sig.generic_parameters;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::Resolver;
    use crate::services::parse_program;

    #[test]
    fn field_type_links_nested_type() {
        let src = r#"
type Inner { i64 x, }
type Outer { Inner inner, }
"#;
        let program = parse_program(src).unwrap();
        let ast: Spanned<AstProgram> = program.clone().into();
        let mut hir = lower_hir_program(&ast);
        normalize_program(&mut hir).unwrap();
        let resolution = Resolver::new().resolve_program(&hir).unwrap();
        let hir_map = HashMap::new();
        let field = resolution
            .items
            .iter()
            .find(|i| i.kind == ItemKind::Field && i.name.contains("inner"))
            .expect("field inner");
        let sig = build_item_signature(field, Some(&resolution), &hir_map, Some(&hir));
        let field_type = sig.field_type.expect("fieldType");
        assert_eq!(field_type.display, "Inner");
        assert!(field_type.ref_item_id.is_some());
    }
}
