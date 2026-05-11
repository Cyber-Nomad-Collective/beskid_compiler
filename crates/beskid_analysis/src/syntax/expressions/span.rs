//! Helpers to build [`SpanInfo`](crate::syntax::SpanInfo) from raw byte ranges in the parser input.

use pest::Span;

use crate::syntax::SpanInfo;

/// Span covering `[start, end)` in `input`, if pest can construct a [`pest::Span`].
pub(crate) fn span_from_bounds(input: &str, start: usize, end: usize) -> Option<SpanInfo> {
    let span = Span::new(input, start, end)?;
    Some(SpanInfo::from_span(&span))
}

/// Span for `op_text` between `start` and `end` in `input` (used for operator tokens).
pub(crate) fn span_from_range(
    input: &str,
    start: usize,
    end: usize,
    op_text: &str,
) -> Option<SpanInfo> {
    let op_start = if op_text.is_empty() {
        start
    } else {
        let between = input.get(start..end)?;
        start + between.find(op_text)?
    };
    let op_end = if op_text.is_empty() {
        end
    } else {
        op_start + op_text.len()
    };
    let span = Span::new(input, op_start, op_end)?;
    Some(SpanInfo::from_span(&span))
}
