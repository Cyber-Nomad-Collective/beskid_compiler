//! Map declaration spans to enum variant names and generic type parameter names (syntax).

use crate::syntax::SpanInfo;
use crate::syntax::{EnumDefinition, FunctionDefinition, Node, Program, Spanned, TypeDefinition};

fn walk_enum_variants(node: &Spanned<Node>, span: SpanInfo) -> Option<Vec<String>> {
    match &node.node {
        Node::EnumDefinition(e) if node.span == span => {
            Some(e.node.variants.iter().map(|v| v.node.name.node.name.clone()).collect())
        }
        Node::InlineModule(im) => {
            for nested in &im.node.items {
                if let Some(v) = walk_enum_variants(nested, span) {
                    return Some(v);
                }
            }
            None
        }
        _ => None,
    }
}

/// When `span` is an enum declaration, return variant names in source order.
pub fn enum_variant_names_for_span(program: &Program, span: SpanInfo) -> Option<Vec<String>> {
    for item in &program.items {
        if let Some(v) = walk_enum_variants(item, span) {
            return Some(v);
        }
    }
    None
}

fn generics_from_type(def: &Spanned<TypeDefinition>) -> Vec<String> {
    def.node.generics.iter().map(|g| g.node.name.clone()).collect()
}

fn generics_from_enum(def: &Spanned<EnumDefinition>) -> Vec<String> {
    def.node.generics.iter().map(|g| g.node.name.clone()).collect()
}

fn generics_from_function(def: &Spanned<FunctionDefinition>) -> Vec<String> {
    def.node.generics.iter().map(|g| g.node.name.clone()).collect()
}

fn walk_generics(node: &Spanned<Node>, span: SpanInfo) -> Option<Vec<String>> {
    match &node.node {
        Node::TypeDefinition(t) if node.span == span => Some(generics_from_type(t)),
        Node::EnumDefinition(e) if node.span == span => Some(generics_from_enum(e)),
        Node::Function(f) if node.span == span => Some(generics_from_function(f)),
        Node::InlineModule(im) => {
            for nested in &im.node.items {
                if let Some(g) = walk_generics(nested, span) {
                    return Some(g);
                }
            }
            None
        }
        _ => None,
    }
}

/// When `span` is a type, enum, or function declaration, return generic type parameter names (may be empty).
pub fn generic_param_names_for_span(program: &Program, span: SpanInfo) -> Option<Vec<String>> {
    for item in &program.items {
        if let Some(g) = walk_generics(item, span) {
            return Some(g);
        }
    }
    None
}
