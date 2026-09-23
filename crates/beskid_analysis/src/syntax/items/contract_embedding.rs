use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Identifier, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// Contract member that embeds another contract by name, optionally supplying type arguments
/// for a generic contract (`Iterator<T>;`).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractEmbedding {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub type_args: Vec<Spanned<Type>>,
}

impl Parsable for ContractEmbedding {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let mut type_args = Vec::new();
        if let Some(generic_args) = inner.next() {
            if generic_args.as_rule() == Rule::GenericArguments {
                type_args = generic_args.into_inner().map(Type::parse).collect::<Result<Vec<_>, _>>()?;
            }
        }

        Ok(Spanned::new(Self { name, type_args }, span))
    }
}
