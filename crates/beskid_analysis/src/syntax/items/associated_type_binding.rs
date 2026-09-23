use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Identifier, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// An implementor's binding for a contract's associated type (`type Item = i64;`), declared
/// inside a `type X : Contract { }` or `impl X : Contract { }` body.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AssociatedTypeBinding {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub ty: Spanned<Type>,
}

impl Parsable for AssociatedTypeBinding {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let ty = Type::parse(inner.next().ok_or(ParseError::missing(Rule::BeskidType))?)?;

        Ok(Spanned::new(Self { name, ty }, span))
    }
}

#[cfg(test)]
mod tests {
    use crate::services::parse_program;
    use crate::syntax::items::Node;

    #[test]
    fn parses_associated_type_binding_in_a_type_body() {
        let program = parse_program("type ArrayIterator : Iterator { type Item = i32; }")
            .expect("an associated-type binding inside a type body should parse");
        let Node::TypeDefinition(def) = &program.node.items[0].node else {
            panic!("expected type definition");
        };
        assert_eq!(def.node.associated_type_bindings.len(), 1);
        assert_eq!(def.node.associated_type_bindings[0].node.name.node.name, "Item");
    }

    #[test]
    fn parses_associated_type_binding_in_an_impl_block() {
        let program = parse_program("impl ArrayIterator : Iterator { type Item = i32; }")
            .expect("an associated-type binding inside an impl block should parse");
        let Node::ImplBlock(def) = &program.node.items[0].node else {
            panic!("expected impl block");
        };
        assert_eq!(def.node.associated_type_bindings.len(), 1);
        assert_eq!(def.node.associated_type_bindings[0].node.name.node.name, "Item");
    }
}
