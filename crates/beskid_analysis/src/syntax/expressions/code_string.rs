//! Fenced `code` literals with optional language tags and compile-time `@{}` holes.

use pest::Parser;
use pest::iterators::Pair;

use crate::parser::{BeskidParser, Rule};
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::SpanInfo;
use crate::syntax::Spanned;

use beskid_ast_derive::AstNode;

/// One segment of a `code` literal body.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CodeStringSegment {
    Text(String),
    Hole(String),
}

/// Parsed `code ```lang ... ``` ` literal.
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CodeStringLiteral {
    pub language: String,
    pub segments: Vec<CodeStringSegment>,
}

impl CodeStringLiteral {
    pub fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        let span = SpanInfo::from_span(&pair.as_span());
        let fence = pair
            .into_inner()
            .find(|child| child.as_rule() == Rule::CodeFence)
            .ok_or_else(|| ParseError::missing(Rule::CodeFence))?;
        let (language, segments) = parse_code_fence(fence)?;
        Ok(Spanned::new(Self { language, segments }, span))
    }

    pub fn materialize_text(&self) -> String {
        let mut out = String::new();
        for segment in &self.segments {
            if let CodeStringSegment::Text(text) = segment {
                out.push_str(text);
            }
        }
        out
    }

    /// Evaluate compile-time `@{}` holes and return the final Beskid source text.
    pub fn materialize_evaluated(&self, eval_hole: impl Fn(&str) -> Result<String, String>) -> Result<String, String> {
        materialize_code_segments(&self.segments, eval_hole)
    }
}

/// Parse a plain Beskid code body containing optional `@{}` compile-time holes.
pub fn parse_plain_code_body(source: &str) -> Result<Vec<CodeStringSegment>, ParseError> {
    let mut pairs = BeskidParser::parse(Rule::CodePlainBody, source).map_err(|_| ParseError::UnexpectedRule {
        expected: Some(Rule::CodePlainBody),
        found: Rule::CodePlainBody,
        span: SpanInfo::default(),
    })?;
    let root = pairs.next().ok_or(ParseError::MissingPair { expected: Rule::CodePlainBody })?;

    let mut segments = Vec::new();
    for child in root.into_inner() {
        for segment in code_plain_body_segments(child) {
            match segment.as_rule() {
                Rule::CodeHole => {
                    let hole = segment.clone();
                    let expr = segment
                        .into_inner()
                        .next()
                        .ok_or_else(|| ParseError::unexpected_rule(hole, Some(Rule::Expression)))?;
                    segments.push(CodeStringSegment::Hole(expr.as_str().trim().to_string()));
                }
                Rule::CodePlainText => {
                    segments.push(CodeStringSegment::Text(segment.as_str().to_string()));
                }
                _ => {}
            }
        }
    }
    Ok(segments)
}

fn code_plain_body_segments<'i>(pair: Pair<'i, Rule>) -> Vec<Pair<'i, Rule>> {
    match pair.as_rule() {
        Rule::CodeHole | Rule::CodePlainText => vec![pair],
        Rule::CodePlainBodyContents => pair.into_inner().collect(),
        _ => Vec::new(),
    }
}

/// Concatenate code segments, evaluating `@{}` holes through `eval_hole`.
pub fn materialize_code_segments(
    segments: &[CodeStringSegment],
    eval_hole: impl Fn(&str) -> Result<String, String>,
) -> Result<String, String> {
    let mut out = String::new();
    for segment in segments {
        match segment {
            CodeStringSegment::Text(text) => out.push_str(text),
            CodeStringSegment::Hole(source) => out.push_str(&eval_hole(source)?),
        }
    }
    Ok(out)
}

fn parse_code_fence(pair: Pair<Rule>) -> Result<(String, Vec<CodeStringSegment>), ParseError> {
    let mut inner = pair.into_inner();
    let open = inner.next().ok_or_else(|| ParseError::missing(Rule::CodeFenceOpen))?;
    let language = open.into_inner().next().map(|tag| tag.as_str().to_string()).unwrap_or_else(|| "beskid".to_string());
    let mut segments = Vec::new();
    for segment in inner {
        for piece in code_fence_body_segments(segment) {
            match piece.as_rule() {
                Rule::CodeHole => {
                    let hole = piece.clone();
                    let expr = piece
                        .into_inner()
                        .next()
                        .ok_or_else(|| ParseError::unexpected_rule(hole, Some(Rule::Expression)))?;
                    segments.push(CodeStringSegment::Hole(expr.as_str().trim().to_string()));
                }
                Rule::CodeFenceChar => {
                    segments.push(CodeStringSegment::Text(piece.as_str().to_string()));
                }
                _ => {}
            }
        }
    }
    Ok((language, segments))
}

fn code_fence_body_segments<'i>(pair: Pair<'i, Rule>) -> Vec<Pair<'i, Rule>> {
    match pair.as_rule() {
        Rule::CodeHole | Rule::CodeFenceChar => vec![pair],
        Rule::CodeFenceBodyContents => pair.into_inner().collect(),
        _ => Vec::new(),
    }
}

impl Parsable for CodeStringLiteral {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        Self::parse(pair)
    }
}
