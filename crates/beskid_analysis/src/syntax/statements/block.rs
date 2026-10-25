use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{SpanInfo, Spanned, Statement};

use beskid_ast_derive::AstNode;

/// Braced sequence of statements.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Block {
    #[ast(children)]
    pub statements: Vec<Spanned<Statement>>,
}

impl Parsable for Block {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let statements = pair
            .into_inner()
            .map(|statement| {
                let at = SpanInfo::from_span(&statement.as_span());
                Statement::parse(statement).map_err(|error| error.located(at))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Spanned::new(Self { statements }, span))
    }
}
