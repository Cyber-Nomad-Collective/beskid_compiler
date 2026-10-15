//! Byte and line/column span metadata for syntax nodes.

use pest::Span;

/// Source span: UTF-8 byte range and 1-based line/column endpoints (inclusive-style reporting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct SpanInfo {
    pub start: usize,
    pub end: usize,
    pub line_col_start: (usize, usize),
    pub line_col_end: (usize, usize),
}

impl SpanInfo {
    /// Builds span metadata from a pest [`Span`].
    ///
    /// Inside [`with_line_index`] for the span's own input the line/column endpoints come from
    /// the precomputed line table; otherwise from pest, which rescans the input from its start.
    pub fn from_span(span: &Span) -> Self {
        let start_pos = span.start_pos();
        let end_pos = span.end_pos();
        let (start, end) = (start_pos.pos(), end_pos.pos());
        let indexed = LINE_INDEX.with(|index| {
            let index = index.borrow();
            let index = index.as_ref().filter(|index| index.covers(span.get_input()))?;
            Some((index.line_col(span.get_input(), start), index.line_col(span.get_input(), end)))
        });
        let (line_col_start, line_col_end) = indexed.unwrap_or_else(|| (start_pos.line_col(), end_pos.line_col()));
        Self { start, end, line_col_start, line_col_end }
    }

    /// Build span from UTF-8 byte offsets into `source` (for non-pest diagnostics).
    pub fn from_byte_range_in_source(source: &str, start: usize, end: usize) -> Self {
        let len = source.len();
        let start = start.min(len);
        let end = end.max(start.saturating_add(1)).min(len);
        Self {
            start,
            end,
            line_col_start: byte_offset_to_line_col(source, start),
            line_col_end: byte_offset_to_line_col(source, end),
        }
    }

    /// Span covering physical line `line_1` (1-based), excluding trailing newline when present.
    pub fn whole_line_in_source(source: &str, line_1: usize) -> Self {
        let target = line_1.saturating_sub(1);
        let mut cur = 0usize;
        for (idx, chunk) in source.split_inclusive('\n').enumerate() {
            if idx == target {
                let end =
                    if chunk.ends_with('\n') && !chunk.is_empty() { cur + chunk.len() - 1 } else { cur + chunk.len() };
                let end = end.max(cur + 1).min(source.len());
                return Self::from_byte_range_in_source(source, cur, end);
            }
            cur += chunk.len();
        }
        if source.is_empty() {
            return Self::from_byte_range_in_source(source, 0, 0);
        }
        Self::from_byte_range_in_source(source, 0, 1.min(source.len()))
    }
}

/// Line-start table of one parser input, identified by the address and length of the exact
/// borrowed `&str` the parser reads. It is installed only for the duration of a parse that
/// borrows that input, so the address cannot be reused by another string while it is current.
struct LineIndex {
    address: usize,
    len: usize,
    /// Byte offset of the first byte of every line after the first (one past each `\n`).
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(input: &str) -> Self {
        let line_starts = input.bytes().enumerate().filter(|(_, byte)| *byte == b'\n').map(|(at, _)| at + 1).collect();
        Self { address: input.as_ptr() as usize, len: input.len(), line_starts }
    }

    fn covers(&self, input: &str) -> bool {
        self.address == input.as_ptr() as usize && self.len == input.len()
    }

    /// The 1-based line/column of byte offset `pos`, equal to `pest::Position::line_col`: `\n`
    /// (also as the end of `\r\n`) ends a line, and the column counts the characters before
    /// `pos` on its line, so a `\r` that `pos` directly follows is a column character.
    fn line_col(&self, input: &str, pos: usize) -> (usize, usize) {
        let line = self.line_starts.partition_point(|start| *start <= pos);
        let line_start = if line == 0 { 0 } else { self.line_starts[line - 1] };
        (line + 1, input[line_start..pos].chars().count() + 1)
    }
}

thread_local! {
    static LINE_INDEX: std::cell::RefCell<Option<LineIndex>> = const { std::cell::RefCell::new(None) };
}

/// Run `parse` with a line table for `input`, so every [`SpanInfo::from_span`] over `input` costs
/// a binary search instead of a scan from the start of the input (which makes parsing quadratic
/// in the source length). Nested calls install their own table and restore the outer one.
pub fn with_line_index<R>(input: &str, parse: impl FnOnce() -> R) -> R {
    struct Restore(Option<LineIndex>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            LINE_INDEX.with(|index| *index.borrow_mut() = previous);
        }
    }
    let previous = LINE_INDEX.with(|index| index.borrow_mut().replace(LineIndex::new(input)));
    let _restore = Restore(previous);
    parse()
}

fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[cfg(test)]
mod line_index_tests {
    use super::{SpanInfo, with_line_index};

    /// The indexed line/column of every position equals pest's own scan, across `\n`, `\r\n`,
    /// lone `\r`, multi-byte characters and the end of input.
    #[test]
    fn indexed_line_col_matches_pest_at_every_position() {
        let input = "a\nb\u{e9}c\r\nd\re\n\n\u{1f600}x\r\n";
        let spans: Vec<pest::Span<'_>> = input
            .char_indices()
            .map(|(at, _)| at)
            .chain([input.len()])
            .flat_map(|start| {
                input.char_indices().map(|(at, _)| at).chain([input.len()]).filter(move |end| *end >= start).map(
                    move |end| pest::Span::new(input, start, end).expect("char-boundary span"),
                )
            })
            .collect();
        let scanned: Vec<SpanInfo> = spans.iter().map(SpanInfo::from_span).collect();
        let indexed: Vec<SpanInfo> = with_line_index(input, || spans.iter().map(SpanInfo::from_span).collect());
        assert_eq!(indexed, scanned);
        // Another input of the same parse falls back to the scan.
        let other = String::from(input);
        let span = pest::Span::new(&other, 3, 9).expect("span");
        assert_eq!(with_line_index(input, || SpanInfo::from_span(&span)), SpanInfo::from_span(&span));
    }
}

/// Types that carry a primary [`SpanInfo`] (typically the whole node’s extent).
pub trait HasSpan {
    fn span(&self) -> &SpanInfo;
}

use crate::syntax::AstNodeId;

/// Syntax node bundled with its source span and dense expanded-tree identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Spanned<T> {
    pub node: T,
    pub span: SpanInfo,
    pub id: AstNodeId,
}

impl<T> Spanned<T> {
    /// Wraps `node` with `span` and an unset node id.
    pub fn new(node: T, span: SpanInfo) -> Self {
        Self { node, span, id: AstNodeId::INVALID }
    }
}

impl<T> HasSpan for Spanned<T> {
    fn span(&self) -> &SpanInfo {
        &self.span
    }
}
