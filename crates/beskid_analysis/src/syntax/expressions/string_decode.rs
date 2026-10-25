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
    Text { value: String, span: SpanInfo },
    RuntimeInterpolation { expression_source: String, span: SpanInfo },
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

/// Decode a `'x'` character literal token (`'\n'`, `'\u{41}'`, `'\''`) to its character.
///
/// Error spans are relative to `token`.
pub fn decode_char_literal_token(token: &str) -> Result<char, ParseError> {
    decode_char_literal_at(token, 0)
}

/// Decode a character literal token that starts at byte `offset` of its source (error spans
/// are then source offsets).
pub(crate) fn decode_char_literal_at(token: &str, offset: usize) -> Result<char, ParseError> {
    let body =
        token.strip_prefix('\'').and_then(|rest| rest.strip_suffix('\'')).filter(|body| !body.is_empty()).ok_or(
            ParseError::UnexpectedRule {
                expected: Some(Rule::CharLiteral),
                found: Rule::CharLiteral,
                span: SpanInfo { start: offset, end: offset + token.len(), ..SpanInfo::default() },
            },
        )?;
    if !body.starts_with('\\') {
        return body.chars().next().ok_or(ParseError::MissingPair { expected: Rule::CharLiteral, span: None });
    }
    let span = SpanInfo { start: offset + 1, end: offset + 1 + body.len(), ..SpanInfo::default() };
    if body == "\\'" {
        return Ok('\'');
    }
    let mut decoded = String::new();
    decode_string_escape(body, span, &mut decoded)?;
    let mut chars = decoded.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Ok(ch),
        // `\${` is a string escape for two characters; a character literal has no use for it.
        _ => Err(ParseError::UnknownStringEscape { span, escape: body.to_string() }),
    }
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
    let mut pairs = BeskidParser::parse(Rule::StringLiteralValue, inner).map_err(|_| ParseError::UnexpectedRule {
        expected: Some(Rule::StringLiteralValue),
        found: Rule::StringLiteralValue,
        span: literal_span,
    })?;
    let root = pairs.next().ok_or(ParseError::MissingPair { expected: Rule::StringLiteralValue, span: None })?;

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
    let mut pairs = BeskidParser::parse(Rule::StringLiteral, token).map_err(|_| ParseError::UnexpectedRule {
        expected: Some(Rule::StringLiteral),
        found: Rule::StringLiteral,
        span: SpanInfo::default(),
    })?;
    let pair = pairs.next().ok_or(ParseError::MissingPair { expected: Rule::StringLiteral, span: None })?;
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
            Rule::StringEscape => {
                let escape_span = offset_span(SpanInfo::from_span(&segment.as_span()), inner_offset);
                decode_string_escape(segment.as_str(), escape_span, &mut text)?;
            }
            Rule::StringInterpolation => {
                let interpolation_span = offset_span(SpanInfo::from_span(&segment.as_span()), inner_offset);
                flush_text_part(parts, &mut text, text_start, interpolation_span.start, input)?;
                let expr = segment
                    .into_inner()
                    .next()
                    .ok_or(ParseError::MissingPair { expected: Rule::InterpolationExpression, span: None })?;
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
    SpanInfo { start: span.start.saturating_add(offset), end: span.end.saturating_add(offset), ..span }
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
    parts.push(StringLiteralPart::Text { value: std::mem::take(text), span });
    Ok(())
}

/// Decode one escape (`raw` includes the backslash) onto `out`; `span` locates it for an error.
fn decode_string_escape(raw: &str, span: SpanInfo, out: &mut String) -> Result<(), ParseError> {
    let unknown = || ParseError::UnknownStringEscape { span, escape: raw.to_string() };
    let body = raw.strip_prefix('\\').ok_or_else(unknown)?;
    let ch = match body {
        "${" => {
            out.push_str("${");
            return Ok(());
        }
        "\"" => '"',
        "\\" => '\\',
        "n" => '\n',
        "t" => '\t',
        "r" => '\r',
        "0" => '\0',
        _ if body.starts_with("u{") && body.ends_with('}') => {
            decode_unicode_escape(raw, &body[2..body.len() - 1], span)?
        }
        _ if body.starts_with('u') => {
            return Err(ParseError::InvalidUnicodeEscape {
                span,
                escape: raw.to_string(),
                reason: "write the code point in braces, as in `\\u{41}`",
            });
        }
        _ => return Err(unknown()),
    };
    out.push(ch);
    Ok(())
}

fn decode_unicode_escape(raw: &str, digits: &str, span: SpanInfo) -> Result<char, ParseError> {
    let invalid = |reason: &'static str| ParseError::InvalidUnicodeEscape { span, escape: raw.to_string(), reason };
    if digits.is_empty() {
        return Err(invalid("no hexadecimal digits"));
    }
    if digits.len() > 6 {
        return Err(invalid("more than six hexadecimal digits"));
    }
    if !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(invalid("only hexadecimal digits may appear between the braces"));
    }
    let value = u32::from_str_radix(digits, 16).map_err(|_| invalid("not a hexadecimal number"))?;
    if (0xD800..=0xDFFF).contains(&value) {
        return Err(invalid("surrogate code points are not characters"));
    }
    char::from_u32(value).ok_or_else(|| invalid("the value is above `10FFFF`"))
}

fn span_from_absolute(input: &str, start: usize, end: usize) -> Result<SpanInfo, ParseError> {
    span_from_bounds(input, start, end).ok_or(ParseError::UnexpectedRule {
        expected: None,
        found: Rule::StringText,
        span: SpanInfo { start, end, ..SpanInfo::default() },
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
    fn preserves_space_only_string_literal() {
        let value = decode_string_literal_token("\" \"").expect("decode");
        assert_eq!(value, " ");
    }

    #[test]
    fn decodes_string_escape_sequences() {
        let value = decode_string_literal_token("\"a\\\"b\\\\c\\${d\"").expect("decode");
        assert_eq!(value, "a\"b\\c${d");
    }

    #[test]
    fn decodes_control_and_unicode_escapes() {
        let value = decode_string_literal_token("\"a\\nb\\tc\\rd\\0e\\u{41}\\u{1F600}\"").expect("decode");
        assert_eq!(value, "a\nb\tc\rd\0eA\u{1F600}");
    }

    #[test]
    fn rejects_unknown_escape_at_its_span() {
        let token = "\"ab\\qc\"";
        let err = decode_string_literal_token(token).expect_err("unknown escape");
        let ParseError::UnknownStringEscape { span, escape } = err else { panic!("unexpected error {err:?}") };
        assert_eq!(escape, "\\q");
        assert_eq!(&token[span.start..span.end], "\\q");
    }

    #[test]
    fn rejects_invalid_unicode_escapes() {
        for (token, escape) in [
            ("\"\\u{D800}\"", "\\u{D800}"),
            ("\"\\u{110000}\"", "\\u{110000}"),
            ("\"\\u{}\"", "\\u{}"),
            ("\"\\u{12345678}\"", "\\u{12345678}"),
            ("\"\\u{zz}\"", "\\u{zz}"),
            ("\"\\u41\"", "\\u"),
        ] {
            let err = decode_string_literal_token(token).expect_err(token);
            let ParseError::InvalidUnicodeEscape { escape: found, span, .. } = err else {
                panic!("{token}: unexpected error {err:?}")
            };
            assert_eq!(found, escape, "{token}");
            assert_eq!(&token[span.start..span.end], escape, "{token}");
        }
    }

    #[test]
    fn decodes_character_literals_and_their_escapes() {
        for (token, expected) in [
            ("'a'", 'a'),
            ("'\\n'", '\n'),
            ("'\\t'", '\t'),
            ("'\\r'", '\r'),
            ("'\\0'", '\0'),
            ("'\\''", '\''),
            ("'\\\\'", '\\'),
            ("'\\\"'", '"'),
            ("'\\u{263A}'", '\u{263A}'),
            ("'é'", 'é'),
        ] {
            assert_eq!(decode_char_literal_token(token), Ok(expected), "{token}");
        }
    }

    #[test]
    fn rejects_unknown_and_invalid_character_escapes() {
        let err = decode_char_literal_token("'\\q'").expect_err("unknown escape");
        let ParseError::UnknownStringEscape { span, escape } = err else { panic!("unexpected error {err:?}") };
        assert_eq!((escape.as_str(), span.start, span.end), ("\\q", 1, 3));
        let err = decode_char_literal_token("'\\${'").expect_err("two-character escape");
        assert!(matches!(err, ParseError::UnknownStringEscape { .. }), "{err:?}");
        let err = decode_char_literal_token("'\\u{D800}'").expect_err("surrogate");
        let ParseError::InvalidUnicodeEscape { span, .. } = err else { panic!("unexpected error {err:?}") };
        assert_eq!((span.start, span.end), (1, 9));
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
