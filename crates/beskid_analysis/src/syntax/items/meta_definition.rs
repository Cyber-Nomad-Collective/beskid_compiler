use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::parse_helpers::{parse_attributes, parse_visibility_or_default};
use crate::syntax::{Attribute, Identifier, SpanInfo, Spanned, TestMetadataEntry, Visibility};

use beskid_ast_derive::AstNode;

/// Top-level `meta name { key = expr; ... }` item (language-meta surface for metaprogramming).
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct MetaDefinition {
    #[ast(children)]
    pub attributes: Vec<Spanned<Attribute>>,
    #[ast(child)]
    pub visibility: Spanned<Visibility>,
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub entries: Vec<Spanned<TestMetadataEntry>>,
}

impl Parsable for MetaDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.clone().into_inner().peekable();
        let attributes = parse_attributes(&mut inner)?;
        let visibility = parse_visibility_or_default(&pair, &mut inner)?;
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let mut entries = Vec::new();
        for item in inner {
            if item.as_rule() == Rule::TestMetadataEntry {
                entries.push(TestMetadataEntry::parse(item)?);
            }
        }
        Ok(Spanned::new(
            Self {
                attributes,
                visibility,
                name,
                entries,
            },
            span,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::BeskidParser;
    use pest::Parser;

    #[test]
    fn parse_meta_definition_roundtrip_shape() {
        let src = r#"pub meta demo { flag = true; }"#;
        let mut pairs = BeskidParser::parse(Rule::MetaDefinition, src).expect("parse");
        let pair = pairs.next().expect("pair");
        let parsed = MetaDefinition::parse(pair).expect("ast");
        assert_eq!(parsed.node.name.node.name, "demo");
        assert_eq!(parsed.node.entries.len(), 1);
    }
}
