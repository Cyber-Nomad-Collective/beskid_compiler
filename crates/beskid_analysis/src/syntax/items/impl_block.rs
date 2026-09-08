use crate::doc::LeadingDocComment;
use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::method_definition::parse_receiver_type;
use crate::syntax::items::parse_helpers::parse_doc_attached_with;
use crate::syntax::{MethodDefinition, Path, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

/// `impl` block for a concrete receiver type and its methods (with per-method leading docs).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImplBlock {
    #[ast(child)]
    pub receiver_type: Spanned<Type>,
    #[ast(children)]
    pub conformances: Vec<Spanned<Path>>,
    #[ast(children)]
    pub methods: Vec<Spanned<MethodDefinition>>,
    #[ast(skip)]
    pub method_docs: Vec<Option<LeadingDocComment>>,
}

impl Parsable for ImplBlock {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();

        let receiver_pair = inner.next().ok_or(ParseError::missing(Rule::ReceiverType))?;
        let receiver_type = parse_receiver_type(receiver_pair)?;

        let mut conformances = Vec::new();
        let mut methods = Vec::new();
        let mut method_docs = Vec::new();
        for item_pair in inner {
            match item_pair.as_rule() {
                Rule::ImplConformanceList => {
                    let path_list = item_pair.into_inner().next().ok_or(ParseError::missing(Rule::PathList))?;
                    conformances = path_list.into_inner().map(Path::parse).collect::<Result<Vec<_>, _>>()?;
                }
                Rule::ImplMethodWithDocs => {
                    let (doc_opt, method) =
                        parse_doc_attached_with(item_pair, Rule::ImplMethodWithDocs, |inner_pair| {
                            MethodDefinition::parse_with_receiver(inner_pair, receiver_type.clone())
                        })?;
                    methods.push(method);
                    method_docs.push(doc_opt);
                }
                _ => return Err(ParseError::unexpected_rule(item_pair, None)),
            }
        }

        Ok(Spanned::new(Self { receiver_type, conformances, methods, method_docs }, span))
    }
}
