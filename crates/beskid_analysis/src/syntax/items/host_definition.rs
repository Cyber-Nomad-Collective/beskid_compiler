use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{Block, Identifier, Parameter, Path, SpanInfo, Spanned, Type};

use beskid_ast_derive::AstNode;

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostDefinition {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub parameters: Vec<Spanned<Parameter>>,
    #[ast(child)]
    pub base_host: Option<Spanned<Path>>,
    #[ast(children)]
    pub body: Vec<Spanned<HostBodyItem>>,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HostBodyItem {
    #[ast(child)]
    Registry(Spanned<RegistryBlock>),
    /// Scope-local registration line (`Impl for IContract;`), same shape as [`RegistryEntry`].
    #[ast(child)]
    Registration(Spanned<RegistryEntry>),
    #[ast(child)]
    Scope(Spanned<ScopeDefinition>),
    #[ast(child)]
    Hook(Spanned<ScopeHook>),
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistryBlock {
    #[ast(children)]
    pub entries: Vec<Spanned<RegistryEntry>>,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistryEntry {
    #[ast(skip)]
    pub lifetime: Option<RegistrationLifetime>,
    #[ast(child)]
    pub implementation: Spanned<Path>,
    #[ast(child)]
    pub target: Option<Spanned<Path>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RegistrationLifetime {
    Single,
    Transient,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeDefinition {
    #[ast(child)]
    pub name: Spanned<Identifier>,
    #[ast(children)]
    pub parameters: Vec<Spanned<Parameter>>,
    #[ast(children)]
    pub body: Vec<Spanned<HostBodyItem>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScopeHookKind {
    Init,
    Dispose,
    Startup,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScopeHook {
    #[ast(skip)]
    pub kind: ScopeHookKind,
    #[ast(children)]
    pub parameters: Vec<Spanned<Parameter>>,
    #[ast(child)]
    pub body: Spanned<Block>,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WithStatement {
    #[ast(child)]
    pub scope_name: Spanned<Identifier>,
    #[ast(children)]
    pub arguments: Vec<Spanned<crate::syntax::Expression>>,
    #[ast(child)]
    pub body: Spanned<Block>,
}

#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LaunchStatement {
    #[ast(child)]
    pub host_path: Spanned<Path>,
    #[ast(children)]
    pub arguments: Vec<Spanned<crate::syntax::Expression>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InjectQualifier {
    Global,
    Parent,
}

impl Parsable for HostDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut parameters = Vec::new();
        let mut base_host = None;
        let mut body = Vec::new();

        for item in inner {
            match item.as_rule() {
                Rule::ParameterList => {
                    parameters = parse_parameter_list(item)?;
                }
                Rule::Path => {
                    base_host = Some(Path::parse(item)?);
                }
                Rule::RegistryBlock | Rule::ScopeDefinition | Rule::ScopeHook => {
                    body.push(parse_body_item_from_inner(item)?);
                }
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }

        Ok(Spanned::new(Self { name, parameters, base_host, body }, span))
    }
}

impl Parsable for RegistryBlock {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let entries = pair
            .into_inner()
            .filter(|entry| entry.as_rule() == Rule::RegistryEntry)
            .map(RegistryEntry::parse)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Spanned::new(Self { entries }, span))
    }
}

impl Parsable for RegistryEntry {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner().peekable();
        let lifetime = if inner.peek().is_some_and(|item| item.as_rule() == Rule::RegistrationLifetime) {
            let life = inner.next().ok_or(ParseError::missing(Rule::RegistrationLifetime))?;
            Some(parse_lifetime(life)?)
        } else {
            None
        };
        let implementation = Path::parse(inner.next().ok_or(ParseError::missing(Rule::Path))?)?;

        let mut target = None;
        for item in inner {
            if item.as_rule() == Rule::RegistrationTarget {
                let target_path = item
                    .into_inner()
                    .find(|inner| inner.as_rule() == Rule::Path)
                    .ok_or(ParseError::missing(Rule::Path))?;
                target = Some(Path::parse(target_path)?);
            }
        }

        Ok(Spanned::new(Self { lifetime, implementation, target }, span))
    }
}

impl Parsable for ScopeDefinition {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;

        let mut parameters = Vec::new();
        let mut body = Vec::new();
        for item in inner {
            match item.as_rule() {
                Rule::ParameterList => {
                    parameters = parse_parameter_list(item)?;
                }
                Rule::RegistryBlock | Rule::ScopeDefinition | Rule::ScopeHook | Rule::RegistryEntry => {
                    body.push(parse_body_item_from_inner(item)?);
                }
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }
        Ok(Spanned::new(Self { name, parameters, body }, span))
    }
}

impl Parsable for ScopeHook {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let name = inner.next().ok_or(ParseError::missing(Rule::ScopeHookName))?;
        let kind = match name.as_str() {
            "init" => ScopeHookKind::Init,
            "dispose" => ScopeHookKind::Dispose,
            "startup" => ScopeHookKind::Startup,
            _ => {
                return Err(ParseError::unexpected_rule(name, Some(Rule::ScopeHookName)));
            }
        };

        let mut parameters = Vec::new();
        let mut body = None;
        for item in inner {
            match item.as_rule() {
                Rule::ParameterList => parameters = parse_parameter_list(item)?,
                Rule::Block => body = Some(Block::parse(item)?),
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }

        Ok(Spanned::new(Self { kind, parameters, body: body.ok_or(ParseError::missing(Rule::Block))? }, span))
    }
}

impl Parsable for WithStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let scope_name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
        let mut arguments = Vec::new();
        let mut body = None;
        for item in inner {
            match item.as_rule() {
                Rule::ArgumentList => {
                    arguments =
                        item.into_inner().map(crate::syntax::Expression::parse).collect::<Result<Vec<_>, _>>()?;
                }
                Rule::Block => body = Some(Block::parse(item)?),
                _ => return Err(ParseError::unexpected_rule(item, None)),
            }
        }
        Ok(Spanned::new(Self { scope_name, arguments, body: body.ok_or(ParseError::missing(Rule::Block))? }, span))
    }
}

impl Parsable for LaunchStatement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let mut inner = pair.into_inner();
        let host_path = Path::parse(inner.next().ok_or(ParseError::missing(Rule::Path))?)?;
        let mut arguments = Vec::new();
        for item in inner {
            if item.as_rule() == Rule::ArgumentList {
                arguments = item.into_inner().map(crate::syntax::Expression::parse).collect::<Result<Vec<_>, _>>()?;
            }
        }
        Ok(Spanned::new(Self { host_path, arguments }, span))
    }
}

fn parse_lifetime(pair: Pair<Rule>) -> Result<RegistrationLifetime, ParseError> {
    match pair.as_str() {
        "single" => Ok(RegistrationLifetime::Single),
        "transient" => Ok(RegistrationLifetime::Transient),
        _ => Err(ParseError::unexpected_rule(pair, Some(Rule::RegistrationLifetime))),
    }
}

fn parse_body_item_from_inner(inner: Pair<Rule>) -> Result<Spanned<HostBodyItem>, ParseError> {
    let span = SpanInfo::from_span(&inner.as_span());
    match inner.as_rule() {
        Rule::RegistryBlock => Ok(Spanned::new(HostBodyItem::Registry(RegistryBlock::parse(inner)?), span)),
        Rule::RegistryEntry => Ok(Spanned::new(HostBodyItem::Registration(RegistryEntry::parse(inner)?), span)),
        Rule::ScopeDefinition => Ok(Spanned::new(HostBodyItem::Scope(ScopeDefinition::parse(inner)?), span)),
        Rule::ScopeHook => Ok(Spanned::new(HostBodyItem::Hook(ScopeHook::parse(inner)?), span)),
        _ => Err(ParseError::unexpected_rule(inner, None)),
    }
}

fn parse_parameter_list(pair: Pair<Rule>) -> Result<Vec<Spanned<Parameter>>, ParseError> {
    pair.into_inner()
        .filter_map(|entry| if entry.as_rule() == Rule::ParameterWithDocs { Some(entry) } else { None })
        .map(|entry| {
            let mut inner = entry.into_inner();
            let first = inner.next().ok_or(ParseError::missing(Rule::Parameter))?;
            let parameter_pair = if first.as_rule() == Rule::DocRun {
                inner.next().ok_or(ParseError::missing(Rule::Parameter))?
            } else {
                first
            };
            Parameter::parse(parameter_pair)
        })
        .collect::<Result<Vec<_>, _>>()
}

pub fn parse_field_inject_parts(
    pair: Pair<Rule>,
) -> Result<(Option<InjectQualifier>, Spanned<Type>, Spanned<Identifier>), ParseError> {
    let mut inner = pair.into_inner();
    let first = inner.next().ok_or(ParseError::missing(Rule::BeskidType))?;
    let (qualifier, ty_pair) = if first.as_rule() == Rule::InjectQualifier {
        let qualifier = match first.as_str().strip_suffix("::") {
            Some("global") => Some(InjectQualifier::Global),
            Some("parent") => Some(InjectQualifier::Parent),
            _ => {
                return Err(ParseError::unexpected_rule(first, Some(Rule::InjectQualifier)));
            }
        };
        (qualifier, inner.next().ok_or(ParseError::missing(Rule::BeskidType))?)
    } else {
        (None, first)
    };
    let ty = Type::parse(ty_pair)?;
    let name = Identifier::parse(inner.next().ok_or(ParseError::missing(Rule::Identifier))?)?;
    Ok((qualifier, ty, name))
}
