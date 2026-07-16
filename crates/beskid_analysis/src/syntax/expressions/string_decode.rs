//! Decode and split `StringContent` segments from parsed `StringLiteral` tokens.

use pest::Parser;
use pest::iterators::Pair;

use crate::parser::{BeskidParser, Rule};
use crate::parsing::error::ParseError;
use crate::syntax::{Literal, SpanInfo};

use super::span::span_from_bounds;

/// One decoded segment inside a string literal token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringLiteralPart {
    Text {
        value: String,
        span: SpanInfo,
    },
    RuntimeInterpolation {
        expression_source: String,
        span: SpanInfo,
    },
}

/// Decode a full `"..."` string literal token into its runtime value.
///
/// Returns an error when the literal contains `${ ... }` interpolation.
pub fn decode_string_literal_token(token: &str) -> Result<String, ParseError> {
    let mut out = String::new();
    for part in split_string_literal_token(token)? {
        match part {
            StringLiteralPart::Text { value, .. } => out.push_str(&value),
            StringLiteralPart::RuntimeInterpolation { span, .. } => {
                return Err(ParseError::UnexpectedRule {
                    expected: Some(Rule::StringText),
                    found: Rule::StringInterpolation,
                    span,
                });
            }
        }
    }
    Ok(out)
}

/// Decode when the token is a plain string literal without `${ ... }` holes.
pub fn try_decode_string_literal_token(token: &str) -> Option<String> {
    decode_string_literal_token(token).ok()
}

/// Decode a [`Literal::String`] token when it has no runtime interpolation.
pub fn try_decode_string_literal(literal: &Literal) -> Option<String> {
    let Literal::String(token) = literal else {
        return None;
    };
    try_decode_string_literal_token(token)
}

/// Split a string literal token into decoded text and `${ ... }` interpolation sites.
pub fn split_string_literal_token(token: &str) -> Result<Vec<StringLiteralPart>, ParseError> {
    let pair = parse_string_literal_pair(token)?;
    let input = pair.as_span().get_input();
    let literal_span = SpanInfo::from_span(&pair.as_span());
    split_string_literal_parts(token, input, literal_span)
}

/// Split using the caller's source buffer and literal span (for desugaring).
pub fn split_string_literal_parts(
    token: &str,
    input: &str,
    literal_span: SpanInfo,
) -> Result<Vec<StringLiteralPart>, ParseError> {
    let inner = string_literal_inner(token)?;
    let mut pairs = BeskidParser::parse(Rule::StringLiteralValue, inner).map_err(|_| {
        ParseError::UnexpectedRule {
            expected: Some(Rule::StringLiteralValue),
            found: Rule::StringLiteralValue,
            span: literal_span,
        }
    })?;
    let root = pairs.next().ok_or(ParseError::MissingPair {
        expected: Rule::StringLiteralValue,
    })?;

    let inner_offset = literal_span.start + 1;

    let mut parts = Vec::new();
    for child in root.into_inner() {
        let mut contents = Vec::new();
        string_literal_value_contents(child, &mut contents);
        for content in contents {
            split_string_content_segment(content, input, inner_offset, &mut parts)?;
        }
    }
    Ok(parts)
}

fn string_literal_value_contents<'i>(pair: Pair<'i, Rule>, out: &mut Vec<Pair<'i, Rule>>) {
    match pair.as_rule() {
        Rule::StringContent => out.push(pair),
        Rule::StringLiteralValueBody => {
            for child in pair.into_inner() {
                string_literal_value_contents(child, out);
            }
        }
        _ => {}
    }
}

fn parse_string_literal_pair(token: &str) -> Result<Pair<'_, Rule>, ParseError> {
    let mut pairs = BeskidParser::parse(Rule::StringLiteral, token).map_err(|_| {
        ParseError::UnexpectedRule {
            expected: Some(Rule::StringLiteral),
            found: Rule::StringLiteral,
            span: SpanInfo::default(),
        }
    })?;
    let pair = pairs.next().ok_or(ParseError::MissingPair {
        expected: Rule::StringLiteral,
    })?;
    if pairs.next().is_some() {
        return Err(ParseError::UnexpectedRule {
            expected: None,
            found: Rule::StringLiteral,
            span: SpanInfo::from_span(&pair.as_span()),
        });
    }
    Ok(pair)
}

fn string_literal_inner(token: &str) -> Result<&str, ParseError> {
    let pair = parse_string_literal_pair(token)?;
    let span = pair.as_span();
    let text = span.as_str();
    if text.len() < 2 {
        return Err(ParseError::UnexpectedRule {
            expected: Some(Rule::StringLiteral),
            found: Rule::StringLiteral,
            span: SpanInfo::from_span(&span),
        });
    }
    Ok(&text[1..text.len() - 1])
}

fn split_string_content_segment(
    content: Pair<Rule>,
    input: &str,
    inner_offset: usize,
    parts: &mut Vec<StringLiteralPart>,
) -> Result<(), ParseError> {
    let content_span = offset_span(SpanInfo::from_span(&content.as_span()), inner_offset);
    let mut text = String::new();
    let mut text_start = content_span.start;

    for segment in content.into_inner() {
        match segment.as_rule() {
            Rule::StringText => text.push_str(segment.as_str()),
            Rule::StringEscape => text.push_str(decode_string_escape(segment.as_str())?),
            Rule::StringInterpolation => {
                let interpolation_span =
                    offset_span(SpanInfo::from_span(&segment.as_span()), inner_offset);
                flush_text_part(
                    parts,
                    &mut text,
                    text_start,
                    interpolation_span.start,
                    input,
                )?;
                let expr = segment.into_inner().next().ok_or(ParseError::MissingPair {
                    expected: Rule::InterpolationExpression,
                })?;
                let expr_span = offset_span(SpanInfo::from_span(&expr.as_span()), inner_offset);
                parts.push(StringLiteralPart::RuntimeInterpolation {
                    expression_source: expr.as_str().trim().to_string(),
                    span: expr_span,
                });
                text.clear();
                text_start = interpolation_span.end;
            }
            _ => {}
        }
    }

    flush_text_part(parts, &mut text, text_start, content_span.end, input)?;
    Ok(())
}

fn offset_span(span: SpanInfo, offset: usize) -> SpanInfo {
    SpanInfo {
        start: span.start.saturating_add(offset),
        end: span.end.saturating_add(offset),
        ..span
    }
}

fn flush_text_part(
    parts: &mut Vec<StringLiteralPart>,
    text: &mut String,
    start: usize,
    end: usize,
    input: &str,
) -> Result<(), ParseError> {
    if text.is_empty() {
        return Ok(());
    }
    let span = span_from_absolute(input, start, end)?;
    parts.push(StringLiteralPart::Text {
        value: std::mem::take(text),
        span,
    });
    Ok(())
}

fn decode_string_escape(raw: &str) -> Result<&'static str, ParseError> {
    match raw {
        "\\\"" => Ok("\""),
        "\\\\" => Ok("\\"),
        "\\${" => Ok("${"),
        _ => Err(ParseError::UnexpectedRule {
            expected: Some(Rule::StringEscape),
            found: Rule::StringEscape,
            span: SpanInfo::default(),
        }),
    }
}

fn span_from_absolute(input: &str, start: usize, end: usize) -> Result<SpanInfo, ParseError> {
    span_from_bounds(input, start, end).ok_or(ParseError::UnexpectedRule {
        expected: None,
        found: Rule::StringText,
        span: SpanInfo {
            start,
            end,
            ..SpanInfo::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_plain_string_literal() {
        let value = decode_string_literal_token("\"line two\"").expect("decode");
        assert_eq!(value, "line two");
    }

    #[test]
    fn decodes_string_escape_sequences() {
        let value = decode_string_literal_token("\"a\\\"b\\\\c\\${d\"").expect("decode");
        assert_eq!(value, "a\"b\\c${d");
    }

    #[test]
    fn splits_runtime_interpolation_segments() {
        let token = "\"hi ${name}!\"";
        let parts = split_string_literal_token(token).expect("split");
        assert_eq!(parts.len(), 3);
        assert!(matches!(&parts[0], StringLiteralPart::Text { value, .. } if value == "hi "));
        assert!(matches!(
            &parts[1],
            StringLiteralPart::RuntimeInterpolation { expression_source, .. }
                if expression_source == "name"
        ));
        assert!(matches!(&parts[2], StringLiteralPart::Text { value, .. } if value == "!"));
    }
}
