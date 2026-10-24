//! Helpers to build [`SpanInfo`](crate::syntax::SpanInfo) from raw byte ranges in the parser input.

use pest::Span;

use crate::syntax::SpanInfo;

/// Span covering `[start, end)` in `input`, if pest can construct a [`pest::Span`].
pub(crate) fn span_from_bounds(input: &str, start: usize, end: usize) -> Option<SpanInfo> {
    let span = Span::new(input, start, end)?;
    Some(SpanInfo::from_span(&span))
}

/// `span` without the whitespace and comments at its end.
///
/// The grammar skips whitespace and comments implicitly between the elements of a rule, so a rule
/// that ends in an optional or repeated part (an operator chain, an operand before an unused `?`)
/// ends after the trivia that preceded the part it did not match. An expression ends at its last
/// token.
pub(crate) fn trim_trailing_trivia(input: &str, span: SpanInfo) -> SpanInfo {
    let bytes = input.as_bytes();
    let (start, end) = (span.start, span.end.min(bytes.len()));
    if end <= start || !(bytes[end - 1].is_ascii_whitespace() || input[..end].ends_with("*/")) {
        return span;
    }
    let mut trimmed = end;
    while trimmed > start && bytes[trimmed - 1].is_ascii_whitespace() {
        trimmed -= 1;
    }
    let line_start = input[..trimmed].rfind('\n').map_or(0, |index| index + 1).max(start);
    let comment_possible = input[..trimmed].ends_with("*/") || input[line_start..trimmed].contains("//");
    if comment_possible {
        // Find the last token by lexing the span; comments and strings make a backward scan ambiguous.
        trimmed = start;
        let mut pos = start;
        while let Some(token) = crate::parsing::syntax_context::token_at(input, pos) {
            if token.start >= end {
                break;
            }
            trimmed = token.end.min(end);
            pos = token.end.max(pos + 1);
        }
    }
    if trimmed == span.end || trimmed <= start {
        return span;
    }
    span_from_bounds(input, start, trimmed).unwrap_or(span)
}

/// Rebase byte/line spans parsed from a substring back into `source`.
pub(crate) fn remap_span(span: SpanInfo, offset: usize, source: &str) -> SpanInfo {
    if offset == 0 {
        return span;
    }
    SpanInfo::from_byte_range_in_source(source, span.start + offset, span.end + offset)
}

/// Span for `op_text` between `start` and `end` in `input` (used for operator tokens).
pub(crate) fn span_from_range(input: &str, start: usize, end: usize, op_text: &str) -> Option<SpanInfo> {
    let op_start = if op_text.is_empty() {
        start
    } else {
        let between = input.get(start..end)?;
        start + between.find(op_text)?
    };
    let op_end = if op_text.is_empty() { end } else { op_start + op_text.len() };
    let span = Span::new(input, op_start, op_end)?;
    Some(SpanInfo::from_span(&span))
}
