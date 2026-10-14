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
    /// `This`: inside a contract body, the eventual implementing type; at an impl site, the
    /// receiver type. A reserved keyword, not an ordinary identifier.
    #[ast(skip)]
    This,
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
                let start = type_name.as_span().start();
                let mut element = Self::parse(type_name)?;
                // Each `ArraySuffix` is one inner rank, spanning from the element type to its own
                // `[]`; the outermost rank is this rule.
                for suffix in inner {
                    if suffix.as_rule() != crate::parser::Rule::ArraySuffix {
                        return Err(crate::parsing::error::ParseError::unexpected_rule(
                            suffix,
                            Some(crate::parser::Rule::ArraySuffix),
                        ));
                    }
                    let suffix_span = suffix.as_span();
                    let rank_span = pest::Span::new(suffix_span.get_input(), start, suffix_span.end())
                        .ok_or(crate::parsing::error::ParseError::missing(crate::parser::Rule::ArraySuffix))?;
                    element = crate::syntax::Spanned::new(
                        Self::Array(Box::new(element)),
                        crate::syntax::SpanInfo::from_span(&rank_span),
                    );
                }
                Self::Array(Box::new(element))
            }
            crate::parser::Rule::ThisType => Self::This,
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

    /// `i64[][]` is an array of `i64[]`: one array rank per `[]`, innermost first, each rank
    /// spanning from the element type to its own suffix.
    #[test]
    fn nested_array_types_parse_one_rank_per_suffix() {
        let source = "i64[][]";
        let outer = parse_type(source);
        let Type::Array(middle) = &outer.node else {
            panic!("expected outer array, got {:?}", outer.node);
        };
        assert_eq!((outer.span.start, outer.span.end), (0, source.len()));
        let Type::Array(element) = &middle.node else {
            panic!("expected `i64[]` elements, got {:?}", middle.node);
        };
        assert_eq!((middle.span.start, middle.span.end), (0, 5));
        assert!(matches!(element.node, Type::Primitive(_)));
        assert_eq!((element.span.start, element.span.end), (0, 3));

        // A one-rank array keeps its pest tree: only the second and later ranks are pairs.
        let single = BeskidParser::parse(Rule::BeskidType, "i64[]").expect("one-rank array").flatten();
        assert!(single.clone().all(|pair| pair.as_rule() != Rule::ArraySuffix), "{single:?}");
        let double = BeskidParser::parse(Rule::BeskidType, source).expect("two-rank array").flatten();
        assert_eq!(double.filter(|pair| pair.as_rule() == Rule::ArraySuffix).count(), 1);
        // The type ends at its last `]`, not at whitespace after it.
        let program = parse_program("unit F() { mut u8[]   bytes = []; i64[][]  rows = []; }").expect("typed locals");
        let formatted = format_program(&program).expect("typed locals format");
        assert!(formatted.contains("u8[] bytes") && formatted.contains("i64[][] rows"), "{formatted}");

        let triple = parse_type("Map<i64>[] [] []");
        let mut ranks = 0;
        let mut current = &triple;
        while let Type::Array(inner) = &current.node {
            ranks += 1;
            current = inner;
        }
        assert_eq!(ranks, 3);
        assert!(matches!(current.node, Type::Complex(_)));

        let program = parse_program(
            "type Grid { i64[][] cells, } i64[][] Rows(string[][] names) { i64[][] local = cells; return local; }",
        )
        .expect("nested array types parse in fields, parameters, locals, and return types");
        let formatted = format_program(&program).expect("nested array types format");
        assert!(formatted.contains("i64[][] cells"), "{formatted}");
        assert!(formatted.contains("i64[][] Rows(string[][] names)"), "{formatted}");
        parse_program(&formatted).expect("formatted nested array types reparse");
    }

    /// Every nesting level parses its generic argument once. Under the former alternation each
    /// level re-parsed its argument per alternative, which never finished for this input.
    #[test]
    fn deeply_nested_generic_arguments_parse_in_linear_time() {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                let source = (0..200).fold("i32".to_owned(), |inner, _| format!("Wrap<{inner}>"));
                let mut ty = parse_type(&source);
                let mut depth = 0;
                loop {
                    let next = match &ty.node {
                        Type::Complex(path) => {
                            let [segment] = path.node.segments.as_slice() else { panic!("one segment per level") };
                            let [argument] = segment.node.type_args.as_slice() else { panic!("one argument per level") };
                            argument.clone()
                        }
                        _ => break,
                    };
                    ty = next;
                    depth += 1;
                }
                assert!(matches!(ty.node, Type::Primitive(_)));
                let _ = sender.send(depth);
            })
            .expect("parser thread");
        let depth = receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("a 200-deep generic type must parse in bounded time");
        assert_eq!(depth, 200);
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
