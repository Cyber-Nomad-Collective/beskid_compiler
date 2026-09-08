use crate::syntax::{Identifier, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;

/// `type Item [= T];` associated-type declaration inside a contract body.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractAssociatedType {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    /// Optional default type binding (`type Item = T;`). An implementor MAY omit
    /// the binding if a default exists; MUST supply one if no default exists.
    #[ast(child)]
    pub default_type: Option<Spanned<Type>>,
}

impl Parsable for ContractAssociatedType {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let default_type = inner.next().map(Type::parse).transpose()?;
        Ok(Spanned::new(Self { name, default_type }, span))
    }
}
