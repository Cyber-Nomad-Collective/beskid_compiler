//! Source comments for the formatter.
//!
//! The grammar treats `//`, `////` and `/* */` comments as implicit whitespace, so the AST does
//! not hold them. The formatter collects them from the parse tree instead: every byte that a leaf
//! token (or an opaque token such as a string literal) does not cover is trivia, and the comments
//! are found only in that trivia. `///` documentation is not trivia here; it is parsed into
//! `LeadingDocComment` values and emitted by the item emitters.

use crate::parser::Rule;
use pest::iterators::Pairs;

/// One comment in the source, with its byte range and exact text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comment {
    pub start: usize,
    pub end: usize,
    /// Exact source text. Line comments lose trailing whitespace only.
    pub text: String,
}

/// Rules whose text is never scanned for comments (their bytes can look like comments).
fn is_opaque(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::StringLiteral
            | Rule::CharLiteral
            | Rule::DocRun
            | Rule::CodeExpression
            | Rule::ClifBlockExpression
            | Rule::StringLiteralValue
    )
}

/// Byte ranges covered by tokens: leaf pairs and opaque pairs.
fn covered_ranges_iter<'i>(pairs: impl Iterator<Item = pest::iterators::Pair<'i, Rule>>, out: &mut Vec<(usize, usize)>) {
    for pair in pairs {
        let span = pair.as_span();
        if is_opaque(pair.as_rule()) {
            out.push((span.start(), span.end()));
            continue;
        }
        let mut inner = pair.clone().into_inner().peekable();
        if inner.peek().is_none() {
            out.push((span.start(), span.end()));
        } else {
            covered_ranges_iter(inner, out);
        }
    }
}

/// Collect every `//`, `////` and `/* */` comment of a successfully parsed source, in source order.
pub(crate) fn collect_comments(source: &str, pairs: Pairs<'_, Rule>) -> Vec<Comment> {
    let mut covered = Vec::new();
    covered_ranges_iter(pairs, &mut covered);
    covered.sort_unstable();

    let mut comments = Vec::new();
    let mut cursor = 0usize;
    for (start, end) in covered {
        if start > cursor {
            scan_gap(source, cursor, start, &mut comments);
        }
        cursor = cursor.max(end);
    }
    if cursor < source.len() {
        scan_gap(source, cursor, source.len(), &mut comments);
    }
    comments
}

/// Find comments in `source[from..to]`, which holds only whitespace, punctuation and comments.
fn scan_gap(source: &str, from: usize, to: usize, out: &mut Vec<Comment>) {
    let bytes = source.as_bytes();
    let mut i = from;
    while i + 1 < to {
        if bytes[i] == b'/' && bytes[i + 1] == b'/' {
            let line_end = source[i..].find('\n').map_or(source.len(), |n| i + n);
            let text = source[i..line_end].trim_end().to_string();
            out.push(Comment { start: i, end: i + text.len(), text });
            i = line_end;
        } else if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            let close = source[i + 2..].find("*/").map_or(source.len(), |n| i + 2 + n + 2);
            out.push(Comment { start: i, end: close, text: source[i..close].to_string() });
            i = close;
        } else {
            i += 1;
        }
    }
}

/// True when `text` holds an empty line: a newline, then only spaces or tabs, then a newline.
pub(crate) fn has_blank_line(text: &str) -> bool {
    let mut newlines = 0;
    for ch in text.chars() {
        match ch {
            '\n' => {
                newlines += 1;
                if newlines >= 2 {
                    return true;
                }
            }
            ' ' | '\t' | '\r' => {}
            _ => newlines = 0,
        }
    }
    false
}

/// True when `text` (between an element end and a comment) keeps the comment on the same line.
pub(crate) fn is_same_line_gap(text: &str) -> bool {
    text.chars().all(|ch| matches!(ch, ' ' | '\t' | ',' | ';'))
}

/// The comparable form of a comment for the preservation check: its text without whitespace.
pub(crate) fn comparable(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}
