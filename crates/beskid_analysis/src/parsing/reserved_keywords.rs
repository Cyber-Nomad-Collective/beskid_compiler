//! Scan raw source for reserved concurrency keywords before or after pest parsing.

use crate::syntax::SpanInfo;

/// Reserved keyword that must not appear in source (parse-time diagnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedKeyword {
    Async,
    Await,
}

/// Find the first reserved keyword token in `source` (word boundaries).
pub fn find_reserved_keyword(source: &str) -> Option<(SpanInfo, ReservedKeyword)> {
    for (keyword, kind) in [("async", ReservedKeyword::Async), ("await", ReservedKeyword::Await)] {
        let mut search_from = 0usize;
        while let Some(rel) = source[search_from..].find(keyword) {
            let start = search_from + rel;
            let end = start + keyword.len();
            if is_word_boundary(source, start, end) {
                return Some((span_for_range(source, start, end), kind));
            }
            search_from = end;
        }
    }
    None
}

fn is_word_boundary(source: &str, start: usize, end: usize) -> bool {
    let before_ok = start == 0 || !source.as_bytes()[start - 1].is_ascii_alphanumeric();
    let after_ok = end >= source.len() || !source.as_bytes()[end].is_ascii_alphanumeric();
    before_ok && after_ok
}

fn span_for_range(source: &str, start: usize, end: usize) -> SpanInfo {
    let prefix = &source[..start];
    let line = prefix.matches('\n').count() + 1;
    let col = prefix
        .rsplit('\n')
        .next()
        .map(|line_prefix| line_prefix.len() + 1)
        .unwrap_or(1);
    let end_prefix = &source[..end];
    let end_line = end_prefix.matches('\n').count() + 1;
    let end_col = end_prefix
        .rsplit('\n')
        .next()
        .map(|line_prefix| line_prefix.len() + 1)
        .unwrap_or(1);
    SpanInfo {
        start,
        end,
        line_col_start: (line, col),
        line_col_end: (end_line, end_col),
    }
}
