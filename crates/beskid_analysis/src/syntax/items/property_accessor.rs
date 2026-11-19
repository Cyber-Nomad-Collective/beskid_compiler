use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::{error::ParseError, parsable::Parsable};
use crate::syntax::{
    Block, Expression, Field, FieldKind, Identifier, MethodDefinition, Parameter, PrimitiveType, SpanInfo, Spanned,
    Type, Visibility,
};

use super::parse_helpers::{parse_attributes, parse_visibility_or_default};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PropertyAccessor {
    Automatic,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PropertyAccessorKind {
    Get,
    Set,
}

impl PropertyAccessorKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Set => "set",
        }
    }

    fn method_prefix(self) -> &'static str {
        match self {
            Self::Get => "$property_get$",
            Self::Set => "$property_set$",
        }
    }

    pub fn method_name(self, property_name: &str) -> String {
        format!("{}{property_name}", self.method_prefix())
    }

    pub fn property_name(self, method_name: &str) -> Option<&str> {
        method_name.strip_prefix(self.method_prefix())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PropertyAccessors {
    pub span: SpanInfo,
    pub get: Option<Spanned<PropertyAccessor>>,
    pub set: Option<Spanned<PropertyAccessor>>,
}

pub(super) fn parse_block_property(
    pair: Pair<Rule>,
    receiver_type: &Spanned<Type>,
) -> Result<(Spanned<Field>, Vec<Spanned<MethodDefinition>>), ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());
    let mut inner = pair.clone().into_inner().peekable();
    let attributes = parse_attributes(&mut inner)?;
    let visibility = parse_visibility_or_default(&pair, &mut inner)?;
    let mut header = inner.next().ok_or(ParseError::missing(Rule::ValueField))?.into_inner();
    let ty = Type::parse(header.next().ok_or(ParseError::missing(Rule::BeskidType))?)?;
    let name = Identifier::parse(header.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
    let accessor_block = inner.next().ok_or(ParseError::missing(Rule::PropertyAccessorBlock))?;
    let property_default = inner.next().map(Expression::parse).transpose()?;
    let mut accessors =
        PropertyAccessors { span: SpanInfo::from_span(&accessor_block.as_span()), get: None, set: None };
    let mut methods = Vec::new();
    for accessor in accessor_block.into_inner() {
        let accessor_span = SpanInfo::from_span(&accessor.as_span());
        let (kind, slot) = match accessor.as_rule() {
            Rule::PropertyGet => (PropertyAccessorKind::Get, &mut accessors.get),
            Rule::PropertySet => (PropertyAccessorKind::Set, &mut accessors.set),
            _ => return Err(ParseError::unexpected_rule(accessor, None)),
        };
        let form = if let Some(body) = accessor.into_inner().next() {
            methods.push(custom_accessor(
                kind,
                receiver_type,
                &name.node.name,
                &ty,
                accessor_span,
                Block::parse(body)?,
            ));
            PropertyAccessor::Custom
        } else {
            PropertyAccessor::Automatic
        };
        *slot = Some(Spanned::new(form, accessor_span));
    }

    let field = Field {
        attributes,
        visibility,
        kind: FieldKind::Value,
        event_capacity: None,
        inject_qualifier: None,
        name,
        ty,
        property_default,
        property_accessors: Some(accessors),
    };
    Ok((Spanned::new(field, span), methods))
}

fn custom_accessor(
    kind: PropertyAccessorKind,
    receiver_type: &Spanned<Type>,
    name: &str,
    ty: &Spanned<Type>,
    span: SpanInfo,
    body: Spanned<Block>,
) -> Spanned<MethodDefinition> {
    let implicit_span = SpanInfo { end: span.start, line_col_end: span.line_col_start, ..span };
    let keyword_end = kind.keyword().len();
    let keyword_span = SpanInfo {
        end: span.start + keyword_end,
        line_col_end: (span.line_col_start.0, span.line_col_start.1 + keyword_end),
        ..span
    };
    let (return_type, parameters) = match kind {
        PropertyAccessorKind::Get => (ty.clone(), Vec::new()),
        PropertyAccessorKind::Set => {
            let parameter = Parameter {
                attributes: Vec::new(),
                bulk: false,
                mutable: false,
                movable: false,
                name: Spanned::new(Identifier { name: "value".to_owned() }, implicit_span),
                ty: Spanned::new(ty.node.clone(), implicit_span),
                default: None,
            };
            let unit = Spanned::new(PrimitiveType::Unit, implicit_span);
            (Spanned::new(Type::Primitive(unit), implicit_span), vec![Spanned::new(parameter, implicit_span)])
        }
    };
    let method = MethodDefinition {
        attributes: Vec::new(),
        visibility: Spanned::new(Visibility::Private, implicit_span),
        receiver_type: Spanned::new(receiver_type.node.clone(), span),
        name: Spanned::new(Identifier { name: kind.method_name(name) }, keyword_span),
        generics: Vec::new(),
        where_bounds: Vec::new(),
        receiver: None,
        parameter_docs: vec![None; parameters.len()],
        parameters,
        return_type: Some(return_type),
        body,
        is_property_getter: false,
        property_accessor: Some(kind),
    };
    Spanned::new(method, span)
}
