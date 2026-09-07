use crate::syntax::{Identifier, Path, PrimitiveType, Spanned};

use beskid_ast_derive::AstNode;

/// Beskid type expression: primitives, paths, arrays, and function types.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Type {
    #[ast(child)]
    Primitive(Spanned<PrimitiveType>),
    #[ast(child)]
    Complex(Spanned<Path>),
    /// A contract-qualified associated type reference such as `Iterator::Item`.
    #[ast(children)]
    Associated { contract: Spanned<Path>, name: Spanned<Identifier> },
    #[ast(child)]
    Array(Box<Spanned<Type>>),
    #[ast(children)]
    Function { return_type: Box<Spanned<Type>>, parameters: Vec<Spanned<Type>> },
}

impl crate::parsing::parsable::Parsable for Type {
    fn parse(
        pair: pest::iterators::Pair<crate::parser::Rule>,
    ) -> Result<crate::syntax::Spanned<Self>, crate::parsing::error::ParseError> {
        let span = crate::syntax::SpanInfo::from_span(&pair.as_span());

        let node = match pair.as_rule() {
            crate::parser::Rule::BeskidType => {
                let inner = pair
                    .into_inner()
                    .next()
                    .ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::TypeName))?;
                let inner_type = Self::parse(inner)?;
                return Ok(crate::syntax::Spanned::new(inner_type.node, span));
            }
            crate::parser::Rule::TypeAtom => {
                let inner = pair
                    .into_inner()
                    .next()
                    .ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::TypeName))?;
                let inner_type = Self::parse(inner)?;
                return Ok(crate::syntax::Spanned::new(inner_type.node, span));
            }
            crate::parser::Rule::FunctionType => {
                let mut inner = pair.into_inner();
                let return_type =
                    inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::TypeName))?;
                let return_type = Self::parse(return_type)?;

                let parameters = inner
                    .next()
                    .map(|list| -> Result<Vec<Spanned<Type>>, crate::parsing::error::ParseError> {
                        list.into_inner().map(Self::parse).collect()
                    })
                    .transpose()?
                    .unwrap_or_default();

                Self::Function { return_type: Box::new(return_type), parameters }
            }
            crate::parser::Rule::ArrowFunctionType => {
                let mut inner = pair.into_inner();
                let first =
                    inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?;

                let (parameters, return_type_pair) = if first.as_rule() == crate::parser::Rule::BeskidTypeList {
                    let parameters = first.into_inner().map(Self::parse).collect::<Result<Vec<_>, _>>()?;
                    let return_type_pair = inner
                        .next()
                        .ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::BeskidType))?;
                    (parameters, return_type_pair)
                } else {
                    (Vec::new(), first)
                };

                let return_type = Self::parse(return_type_pair)?;
                Self::Function { return_type: Box::new(return_type), parameters }
            }
            crate::parser::Rule::TypeName => {
                let mut inner = pair.into_inner();
                let first = inner
                    .next()
                    .ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::PrimitiveType))?;

                match first.as_rule() {
                    crate::parser::Rule::PrimitiveType => {
                        let primitive = crate::syntax::PrimitiveType::parse(first)?;
                        Self::Primitive(primitive)
                    }
                    crate::parser::Rule::Path => {
                        let path = crate::syntax::Path::parse(first)?;
                        Self::Complex(path)
                    }
                    _ => {
                        return Err(crate::parsing::error::ParseError::unexpected_rule(
                            first,
                            Some(crate::parser::Rule::TypeName),
                        ));
                    }
                }
            }
            crate::parser::Rule::ArrayType => {
                let mut inner = pair.into_inner();
                let type_name =
                    inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::TypeName))?;
                let inner_type = Self::parse(type_name)?;
                Self::Array(Box::new(inner_type))
            }
            crate::parser::Rule::AssociatedTypeRef => {
                let mut inner = pair.into_inner();
                let contract = crate::syntax::Path::parse(
                    inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::Path))?,
                )?;
                let name = crate::syntax::Identifier::parse(
                    inner.next().ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::Identifier))?,
                )?;
                Self::Associated { contract, name }
            }
            crate::parser::Rule::PrimitiveType => {
                let primitive = crate::syntax::PrimitiveType::parse(pair)?;
                Self::Primitive(primitive)
            }
            crate::parser::Rule::Path => {
                let path = crate::syntax::Path::parse(pair)?;
                Self::Complex(path)
            }
            _ => {
                return Err(crate::parsing::error::ParseError::unexpected_rule(
                    pair,
                    Some(crate::parser::Rule::BeskidType),
                ));
            }
        };

        Ok(crate::syntax::Spanned::new(node, span))
    }
}

#[cfg(test)]
mod tests {
    use crate::format::format_program;
    use crate::parser::{BeskidParser, Rule};
    use crate::parsing::parsable::Parsable;
    use crate::services::parse_program;
    use crate::syntax::{Spanned, items::Node};
    use pest::Parser;

    use super::Type;

    #[test]
    fn parses_associated_type_reference_as_a_beskid_type() {
        let program = parse_program("Iterator::Item Current() { return; }")
            .expect("associated type reference should parse as a function return type");
        let Node::Function(function) = &program.node.items[0].node else {
            panic!("expected function item");
        };
        let Some(return_type) = &function.node.return_type else {
            panic!("expected function return type");
        };
        let Type::Associated { contract, name } = &return_type.node else {
            panic!("expected associated type AST node");
        };

        assert_eq!(contract.node.segments[0].node.name.node.name, "Iterator");
        assert_eq!(name.node.name, "Item");
        assert!(
            format_program(&program).expect("associated type reference should format").contains("Iterator::Item"),
            "formatter must preserve the associated-type separator"
        );
    }

    #[test]
    fn rejects_incomplete_or_chained_associated_type_references() {
        for source in ["Iterator::", "::Item", "Iterator::Item::Nested"] {
            assert!(
                parse_program(&format!("{source} Current() {{ return; }}")).is_err(),
                "{source:?} must not be accepted as an associated type reference"
            );
        }
    }

    #[test]
    fn associated_type_references_compose_in_arrays_functions_and_generic_arguments() {
        let array = parse_type("Iterator::Item[]");
        let Type::Array(element) = &array.node else {
            panic!("expected associated type array");
        };
        assert_associated_type(element, "Iterator", "Item", 0, 14);
        assert_eq!(array.span.start, 0);
        assert_eq!(array.span.end, "Iterator::Item[]".len());

        let function = parse_type("Iterator::Item(i64)");
        let Type::Function { return_type, parameters } = &function.node else {
            panic!("expected associated type function return");
        };
        assert_associated_type(return_type, "Iterator", "Item", 0, 14);
        assert!(matches!(parameters.as_slice(), [parameter] if matches!(parameter.node, Type::Primitive(_))));

        let generic = parse_type("Container<Iterator::Item>");
        let Type::Complex(path) = &generic.node else {
            panic!("expected generic path type");
        };
        let [segment] = path.node.segments.as_slice() else {
            panic!("expected one path segment");
        };
        let [argument] = segment.node.type_args.as_slice() else {
            panic!("expected one generic argument");
        };
        assert_associated_type(argument, "Iterator", "Item", 10, 24);
    }

    #[test]
    fn associated_type_references_round_trip_through_the_formatter() {
        let source = "Iterator::Item[] ArrayCurrent() { return; } Iterator::Item(i64) Mapper() { return; } Container<Iterator::Item> Wrapped() { return; }";
        let formatted = format_program(&parse_program(source).expect("compositional associated types should parse"))
            .expect("compositional associated types should format");

        assert!(formatted.contains("Iterator::Item[]"));
        assert!(formatted.contains("Iterator::Item(i64)"));
        assert!(formatted.contains("Container<Iterator::Item>"));
        parse_program(&formatted).expect("formatted associated types should reparse");
    }

    fn parse_type(source: &str) -> Spanned<Type> {
        let pair = BeskidParser::parse(Rule::BeskidType, source)
            .expect("Beskid type should parse")
            .next()
            .expect("Beskid type pair");
        assert_eq!(pair.as_span().as_str(), source, "BeskidType must consume the complete direct input");
        Type::parse(pair).expect("Beskid type AST should parse")
    }

    fn assert_associated_type(
        ty: &Spanned<Type>,
        contract_name: &str,
        associated_name: &str,
        start: usize,
        end: usize,
    ) {
        let Type::Associated { contract, name } = &ty.node else {
            panic!("expected associated type AST node");
        };
        assert_eq!(contract.node.segments[0].node.name.node.name, contract_name);
        assert_eq!(name.node.name, associated_name);
        assert_eq!(ty.span.start, start);
        assert_eq!(ty.span.end, end);
        assert_eq!(contract.span.start, start);
        assert_eq!(contract.span.end, start + contract_name.len());
        assert_eq!(name.span.start, start + contract_name.len() + 2);
        assert_eq!(name.span.end, end);
    }
}
