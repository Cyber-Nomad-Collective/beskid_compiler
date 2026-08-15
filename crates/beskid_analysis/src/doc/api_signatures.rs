//! Syntax-derived signatures and type annotations for `api.json` rows.

use crate::resolve::items::{ItemInfo, ItemKind};
use crate::resolve::Resolution;
use crate::syntax::{
    ContractMethodSignature, ContractNode, EnumDefinition, EnumVariant, Field, FunctionDefinition, MethodDefinition,
    Node, Parameter, Program, SpanInfo, Spanned, TypeDefinition,
};

use super::api_snapshot::{ApiGenericParameterDoc, ApiItemSignature, ApiParameterDoc, ApiTypeAnnotation};
use super::qualified_names::{display_name_for_item, module_path_for_item};
use super::type_format::type_annotation_for_type;

enum SyntaxDeclAtSpan<'a> {
    Function(&'a Spanned<FunctionDefinition>),
    Method(&'a Spanned<MethodDefinition>),
    Type(&'a Spanned<TypeDefinition>),
    Enum(&'a Spanned<EnumDefinition>),
    EnumVariant(&'a Spanned<EnumVariant>),
    Field(&'a Spanned<Field>),
    Parameter(&'a Spanned<Parameter>),
    ContractMethod(&'a Spanned<ContractMethodSignature>),
}

fn spans_equal(a: SpanInfo, b: SpanInfo) -> bool {
    a.start == b.start && a.end == b.end
}

fn find_decl_at_span(program: &Program, span: SpanInfo) -> Option<SyntaxDeclAtSpan<'_>> {
    program.items.iter().find_map(|item| find_decl_in_node(item, span))
}

fn find_parameter<'a>(
    mut parameters: impl Iterator<Item = &'a Spanned<Parameter>>,
    span: SpanInfo,
) -> Option<SyntaxDeclAtSpan<'a>> {
    parameters.find(|parameter| spans_equal(parameter.span, span)).map(SyntaxDeclAtSpan::Parameter)
}

fn find_field<'a>(
    mut fields: impl Iterator<Item = &'a Spanned<Field>>,
    span: SpanInfo,
) -> Option<SyntaxDeclAtSpan<'a>> {
    fields.find(|field| spans_equal(field.span, span)).map(SyntaxDeclAtSpan::Field)
}

fn find_contract_method<'a>(items: &'a [Spanned<ContractNode>], span: SpanInfo) -> Option<SyntaxDeclAtSpan<'a>> {
    for item in items {
        let ContractNode::MethodSignature(method) = &item.node else {
            continue;
        };
        if spans_equal(item.span, span) {
            return Some(SyntaxDeclAtSpan::ContractMethod(method));
        }
        if let Some(parameter) = find_parameter(method.node.parameters.iter(), span) {
            return Some(parameter);
        }
    }
    None
}

fn find_decl_in_node<'a>(item: &'a Spanned<Node>, span: SpanInfo) -> Option<SyntaxDeclAtSpan<'a>> {
    match &item.node {
        Node::Function(def) => {
            if spans_equal(item.span, span) {
                Some(SyntaxDeclAtSpan::Function(def))
            } else {
                find_parameter(def.node.parameters.iter(), span)
            }
        }
        Node::Method(def) => {
            if spans_equal(item.span, span) {
                Some(SyntaxDeclAtSpan::Method(def))
            } else {
                find_parameter(def.node.parameters.iter(), span)
            }
        }
        Node::TypeDefinition(def) => {
            if spans_equal(item.span, span) {
                return Some(SyntaxDeclAtSpan::Type(def));
            }
            find_field(def.node.fields.iter(), span).or_else(|| {
                def.node.methods.iter().find_map(|method| {
                    if spans_equal(method.span, span) {
                        Some(SyntaxDeclAtSpan::Method(method))
                    } else {
                        find_parameter(method.node.parameters.iter(), span)
                    }
                })
            })
        }
        Node::EnumDefinition(def) => {
            if spans_equal(item.span, span) {
                return Some(SyntaxDeclAtSpan::Enum(def));
            }
            def.node.variants.iter().find_map(|variant| {
                if spans_equal(variant.span, span) {
                    Some(SyntaxDeclAtSpan::EnumVariant(variant))
                } else {
                    find_field(variant.node.fields.iter(), span)
                }
            })
        }
        Node::ExtendTypeDefinition(def) => def.node.methods.iter().find_map(|method| {
            if spans_equal(method.span, span) {
                Some(SyntaxDeclAtSpan::Method(method))
            } else {
                find_parameter(method.node.parameters.iter(), span)
            }
        }),
        Node::ContractDefinition(def) => find_contract_method(&def.node.items, span),
        Node::InlineModule(module) => module.node.items.iter().find_map(|nested| find_decl_in_node(nested, span)),
        _ => None,
    }
}

fn parameter_modifier(parameter: &Parameter) -> Option<String> {
    parameter.mutable.then(|| "mut".to_string())
}

fn generic_parameters_from_names(names: impl Iterator<Item = String>) -> Vec<ApiGenericParameterDoc> {
    names.map(|name| ApiGenericParameterDoc { name }).collect()
}

fn callable_parameters(parameters: &[Spanned<Parameter>], resolution: Option<&Resolution>) -> Vec<ApiParameterDoc> {
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
    return_type: Option<&Spanned<crate::syntax::Type>>,
    resolution: Option<&Resolution>,
) -> Option<ApiTypeAnnotation> {
    return_type.map(|ty| type_annotation_for_type(ty, resolution))
}

fn callable_signature(
    name: &str,
    parameters: &[Spanned<Parameter>],
    return_type: Option<&Spanned<crate::syntax::Type>>,
    resolution: Option<&Resolution>,
    include_mutability: bool,
) -> (Option<ApiTypeAnnotation>, Vec<ApiParameterDoc>, String) {
    let return_type = return_annotation(return_type, resolution);
    let parameters = callable_parameters(parameters, resolution);
    let result = return_type.as_ref().map(|ty| ty.display.clone()).unwrap_or_else(|| "unit".to_string());
    let parameters = parameters
        .into_iter()
        .map(|parameter| {
            let modifier = include_mutability
                .then(|| parameter.modifier.as_deref().unwrap_or_default())
                .filter(|modifier| !modifier.is_empty())
                .map(|modifier| format!("{modifier} "))
                .unwrap_or_default();
            (parameter, modifier)
        })
        .collect::<Vec<_>>();
    let text = parameters
        .iter()
        .map(|(parameter, modifier)| format!("{modifier}{} {}", parameter.ty.display, parameter.name))
        .collect::<Vec<_>>()
        .join(", ");
    (return_type, parameters.into_iter().map(|(parameter, _)| parameter).collect(), format!("{result} {name}({text})"))
}

fn build_from_decl(decl: SyntaxDeclAtSpan<'_>, item: &ItemInfo, resolution: Option<&Resolution>) -> ApiItemSignature {
    let mut signature = ApiItemSignature {
        display_name: Some(display_name_for_item(item)),
        module_path: resolution
            .map(|resolution| module_path_for_item(item, &resolution.module_graph))
            .unwrap_or_default(),
        ..Default::default()
    };
    match decl {
        SyntaxDeclAtSpan::Function(def) => {
            let (return_type, parameters, text) = callable_signature(
                &def.node.name.node.name,
                &def.node.parameters,
                def.node.return_type.as_ref(),
                resolution,
                true,
            );
            signature.return_type = return_type;
            signature.parameters = parameters;
            signature.generic_parameters =
                generic_parameters_from_names(def.node.generics.iter().map(|generic| generic.node.name.clone()));
            signature.signature = Some(text);
        }
        SyntaxDeclAtSpan::Method(def) => {
            let (return_type, parameters, text) = callable_signature(
                &def.node.name.node.name,
                &def.node.parameters,
                def.node.return_type.as_ref(),
                resolution,
                true,
            );
            signature.return_type = return_type;
            signature.parameters = parameters;
            signature.signature = Some(text);
        }
        SyntaxDeclAtSpan::Type(def) => {
            let generics = def.node.generics.iter().map(|generic| generic.node.name.clone());
            signature.generic_parameters = generic_parameters_from_names(generics.clone());
            let generic_text = generics.collect::<Vec<_>>().join(", ");
            signature.signature = Some(if generic_text.is_empty() {
                format!("type {}", def.node.name.node.name)
            } else {
                format!("type {}<{generic_text}>", def.node.name.node.name)
            });
        }
        SyntaxDeclAtSpan::Enum(def) => {
            let generics = def.node.generics.iter().map(|generic| generic.node.name.clone());
            signature.generic_parameters = generic_parameters_from_names(generics.clone());
            let generic_text = generics.collect::<Vec<_>>().join(", ");
            signature.signature = Some(if generic_text.is_empty() {
                format!("enum {}", def.node.name.node.name)
            } else {
                format!("enum {}<{generic_text}>", def.node.name.node.name)
            });
        }
        SyntaxDeclAtSpan::EnumVariant(variant) => {
            signature.signature = Some(format!("enum variant {}", variant.node.name.node.name));
        }
        SyntaxDeclAtSpan::Field(field) => {
            let field_type = type_annotation_for_type(&field.node.ty, resolution);
            signature.signature = Some(format!("{} {}", field_type.display, field.node.name.node.name));
            signature.field_type = Some(field_type);
        }
        SyntaxDeclAtSpan::Parameter(parameter) => {
            let field_type = type_annotation_for_type(&parameter.node.ty, resolution);
            signature.signature = Some(format!("{} {}", field_type.display, parameter.node.name.node.name));
            signature.field_type = Some(field_type);
        }
        SyntaxDeclAtSpan::ContractMethod(method) => {
            let (return_type, parameters, text) = callable_signature(
                &method.node.name.node.name,
                &method.node.parameters,
                method.node.return_type.as_ref(),
                resolution,
                false,
            );
            signature.return_type = return_type;
            signature.parameters = parameters;
            signature.signature = Some(text);
        }
    }
    signature
}

/// Build syntax-derived signature fields for one resolved item.
pub fn build_item_signature(
    item: &ItemInfo,
    resolution: Option<&Resolution>,
    program: &Spanned<Program>,
) -> ApiItemSignature {
    let base = ApiItemSignature {
        display_name: Some(display_name_for_item(item)),
        module_path: resolution
            .map(|resolution| module_path_for_item(item, &resolution.module_graph))
            .unwrap_or_default(),
        ..Default::default()
    };
    let Some(decl) = find_decl_at_span(&program.node, item.span) else {
        return if matches!(item.kind, ItemKind::Statement) {
            ApiItemSignature { signature: Some("statement".to_string()), ..base }
        } else {
            base
        };
    };
    build_from_decl(decl, item, resolution)
}

/// Apply [`ApiItemSignature`] fields onto an [`super::api_snapshot::ApiDocItem`].
pub fn apply_signature_to_item(item: &mut super::api_snapshot::ApiDocItem, signature: ApiItemSignature) {
    item.display_name = signature.display_name;
    item.module_path = signature.module_path;
    item.signature = signature.signature;
    item.field_type = signature.field_type;
    item.return_type = signature.return_type;
    item.parameters = signature.parameters;
    item.generic_parameters = signature.generic_parameters;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::Resolver;
    use crate::services::parse_program;

    #[test]
    fn field_type_links_nested_type() {
        let source = r#"
type Inner { i64 x, }
type Outer { Inner inner, }
"#;
        let program = parse_program(source).unwrap();
        let resolution = Resolver::new().resolve_program(&program).unwrap();
        let field = resolution
            .items
            .iter()
            .find(|item| item.kind == ItemKind::Field && item.name.contains("inner"))
            .expect("field inner");
        let signature = build_item_signature(field, Some(&resolution), &program);
        let field_type = signature.field_type.expect("fieldType");
        assert_eq!(field_type.display, "Inner");
        assert!(field_type.ref_item_id.is_some());
    }
}
