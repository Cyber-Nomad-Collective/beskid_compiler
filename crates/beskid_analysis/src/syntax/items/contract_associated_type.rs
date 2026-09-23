use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Identifier, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Contract member declaring an associated type (`type Item;`), optionally with a default
/// (`type Item = T;`). The bare name (`Item`) is in scope inside the contract's own method
/// signatures, exactly like a generic parameter; an implementor binds it via
/// `type Item = Concrete;` in its own body (`AssociatedTypeBinding`), or relies on the default
/// if one is declared.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractAssociatedType {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(child)]
    pub default: Option<Spanned<Type>>,
}

impl Parsable for ContractAssociatedType {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let default = inner.next().map(Type::parse).transpose()?;

        Ok(Spanned::new(Self { name, default }, span))
    }
}

#[cfg(test)]
mod tests {
    use crate::services::parse_program;
    use crate::syntax::items::Node;
    use crate::syntax::ContractNode;

    #[test]
    fn parses_associated_type_without_default() {
        let program = parse_program("contract Iterator { type Item; Item Current(); }")
            .expect("associated type declaration without a default should parse");
        let Node::ContractDefinition(contract) = &program.node.items[0].node else {
            panic!("expected contract definition");
        };
        let ContractNode::AssociatedType(assoc) = &contract.node.items[0].node else {
            panic!("expected associated type declaration as the first contract item");
        };
        assert_eq!(assoc.node.name.node.name, "Item");
        assert!(assoc.node.default.is_none());
    }

    #[test]
    fn parses_associated_type_with_default() {
        let program = parse_program("contract Box<T> { type Item = T; Item Get(); }")
            .expect("associated type declaration with a default should parse");
        let Node::ContractDefinition(contract) = &program.node.items[0].node else {
            panic!("expected contract definition");
        };
        let ContractNode::AssociatedType(assoc) = &contract.node.items[0].node else {
            panic!("expected associated type declaration as the first contract item");
        };
        assert_eq!(assoc.node.name.node.name, "Item");
        assert!(assoc.node.default.is_some());
    }
}
