//! Map resolution item spans to callable parameter lists and return shape (syntax).

use crate::syntax::SpanInfo;
use crate::syntax::{
    ContractMethodSignature, ContractNode, FunctionDefinition, MethodDefinition, Node, PrimitiveType, Program, Spanned,
    Type,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallableSignatures {
    pub param_names: Vec<String>,
    pub returns_unit: bool,
}

fn type_is_unit(ty: &Type) -> bool {
    matches!(ty, Type::Primitive(p) if p.node == PrimitiveType::Unit)
}

/// One-line source spelling of a function's signature, as hovers and signature help show it:
/// `unit Check(bool ok, string because = "")`. Parameters print as the formatter writes them, so
/// attributes and default values appear.
pub fn function_signature_label(def: &FunctionDefinition) -> String {
    let emitter = crate::format::Emitter::new();
    let result =
        def.return_type.as_ref().and_then(|ty| emitter.write(&ty.node).ok()).unwrap_or_else(|| "unit".to_string());
    let generics = if def.generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", def.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>().join(", "))
    };
    let parameters = def
        .parameters
        .iter()
        .map(|parameter| emitter.write(&parameter.node).unwrap_or_else(|_| parameter.node.name.node.name.clone()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{result} {}{generics}({parameters})", def.name.node.name)
}

fn from_function(def: &Spanned<FunctionDefinition>) -> CallableSignatures {
    CallableSignatures {
        param_names: def.node.parameters.iter().map(|p| p.node.name.node.name.clone()).collect(),
        returns_unit: def.node.return_type.as_ref().is_none_or(|t| type_is_unit(&t.node)),
    }
}

fn from_method(def: &Spanned<MethodDefinition>) -> CallableSignatures {
    CallableSignatures {
        param_names: def.node.parameters.iter().map(|p| p.node.name.node.name.clone()).collect(),
        returns_unit: def.node.return_type.as_ref().is_none_or(|t| type_is_unit(&t.node)),
    }
}

fn from_contract_method(sig: &Spanned<ContractMethodSignature>) -> CallableSignatures {
    CallableSignatures {
        param_names: sig.node.parameters.iter().map(|p| p.node.name.node.name.clone()).collect(),
        returns_unit: sig.node.return_type.as_ref().is_none_or(|t| type_is_unit(&t.node)),
    }
}

fn walk_contract_items(items: &[Spanned<ContractNode>], span: SpanInfo) -> Option<CallableSignatures> {
    for item in items {
        if let ContractNode::MethodSignature(sig) = &item.node
            && sig.span == span
        {
            return Some(from_contract_method(sig));
        }
    }
    None
}

fn walk_node(node: &Spanned<Node>, span: SpanInfo) -> Option<CallableSignatures> {
    match &node.node {
        Node::Function(f) if node.span == span => Some(from_function(f)),
        Node::Method(m) if node.span == span => Some(from_method(m)),
        Node::ExtendTypeDefinition(extension) => {
            for method in &extension.node.methods {
                if method.span == span {
                    return Some(from_method(method));
                }
            }
            None
        }
        Node::ContractDefinition(c) => {
            if let Some(s) = walk_contract_items(&c.node.items, span) {
                return Some(s);
            }
            None
        }
        Node::InlineModule(im) => {
            for nested in &im.node.items {
                if let Some(s) = walk_node(nested, span) {
                    return Some(s);
                }
            }
            None
        }
        _ => None,
    }
}

/// When `span` is the declaration span of a function, method, or contract method, return parameters and whether the return type is `unit`.
pub fn callable_signatures_for_span(program: &Program, span: SpanInfo) -> Option<CallableSignatures> {
    for item in &program.items {
        if let Some(s) = walk_node(item, span) {
            return Some(s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::parse_program;

    #[test]
    fn a_function_label_shows_attributes_and_default_values() {
        let program = parse_program(
            "pub unit Equal<T>(T actual, T expected, string because = \"\", [CallerLineNumber] i64 line = 0) { return; }",
        )
        .unwrap();
        let Node::Function(function) = &program.node.items[0].node else { panic!("function") };
        assert_eq!(
            function_signature_label(&function.node),
            "unit Equal<T>(T actual, T expected, string because = \"\", [CallerLineNumber] i64 line = 0)"
        );
    }
}
