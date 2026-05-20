use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::method_definition::parse_receiver_type;
use crate::syntax::items::parse_helpers::parse_doc_attached_with;
use crate::syntax::{MethodDefinition, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// `extend type T { ... }` block preserving source grouping for extension semantics.
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub struct ExtendTypeDefinition {
    #[ast(child)]
    pub target_type: Spanned<Type>,
    #[ast(children)]
    pub methods: Vec<Spanned<MethodDefinition>>,
    #[ast(skip)]
    pub method_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for ExtendTypeDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();

        let target_pair = inner
            .next()
            .ok_or(ParseError::missing(Rule::ReceiverType))?;
        let target_type = parse_receiver_type(target_pair)?;

        let mut methods = Vec::new();
        let mut method_docs = Vec::new();
        for method_pair in inner {
            let (doc_opt, method) =
                parse_doc_attached_with(method_pair, Rule::ImplMethodWithDocs, |inner_pair| {
                    MethodDefinition::parse_with_receiver(inner_pair, target_type.clone())
                })?;
            methods.push(method);
            method_docs.push(doc_opt);
        }

        Ok(Spanned::new(
            Self {
                target_type,
                methods,
                method_docs,
            },
            span,
        ))
    }
}
