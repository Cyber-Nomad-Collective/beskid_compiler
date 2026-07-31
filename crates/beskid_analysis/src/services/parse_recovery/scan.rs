//! Shared scanner and token helper utilities.

use super::syntax_primitives;
use crate::parser::Rule;
use pest::error::InputLocation;

/// Extract normalized parse byte position from pest errors.
pub(crate) fn error_byte_pos(parse_error: &pest::error::Error<Rule>) -> usize {
    match parse_error.location {
        InputLocation::Pos(pos) => pos,
        InputLocation::Span((start, _)) => start,
    }
}

pub(crate) fn skip_ws(source: &str, from: usize) -> usize {
    let bytes = source.as_bytes();
    let mut pos = from.min(source.len());
    while pos < source.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

/// Backward keyword search with token-boundary checks.
pub(crate) fn find_keyword_backward(source: &str, through: usize, keyword: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let kw = keyword.as_bytes();
    if through < kw.len() {
        return None;
    }
    let mut start = through.min(source.len()) - kw.len();
    loop {
        if &bytes[start..start + kw.len()] == kw {
            let before_ok = start == 0 || !is_ident_continue(bytes[start - 1]);
            let end = start + kw.len();
            let after_ok = end >= source.len() || !is_ident_continue(bytes[end]);
            if before_ok && after_ok {
                return Some(start);
            }
        }
        if start == 0 {
            break;
        }
        start -= 1;
    }
    None
}

/// Predicate for identifier-like bytes in parser tokens.
pub(crate) fn is_ident_start(bytes: u8) -> bool {
    bytes.is_ascii_alphabetic() || bytes == b'_'
}

pub(crate) fn is_ident_continue(bytes: u8) -> bool {
    bytes.is_ascii_alphanumeric() || bytes == b'_'
}

pub(crate) fn is_open_delimiter_byte(byte: u8) -> bool {
    matches!(byte, b'(' | b'[' | b'{' | b'<')
}

pub(crate) fn is_close_delimiter_byte(byte: u8) -> bool {
    matches!(byte, b')' | b']' | b'}' | b'>')
}

pub(crate) fn is_delimiter_byte(byte: u8) -> bool {
    is_open_delimiter_byte(byte) || is_close_delimiter_byte(byte) || matches!(byte, b',' | b';' | b':' | b'.' | b'`')
}

pub(crate) fn is_operator_byte(byte: u8) -> bool {
    matches!(byte, b'=' | b'<' | b'>' | b'!' | b'+' | b'-' | b'*' | b'/' | b'&' | b'|' | b'?' | b'%')
}

const MULTI_CHAR_OPERATORS: &[&str] = &[
    "===", "!==", "<<", ">>", "=>", "==", "!=", ">=", "<=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "::",
    "++", "--", "->",
];

fn operator_token_len(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let remaining = source.len() - start;
    for op in MULTI_CHAR_OPERATORS.iter() {
        if remaining < op.len() {
            continue;
        }
        if &bytes[start..start + op.len()] == op.as_bytes() {
            return Some(op.len());
        }
    }
    None
}

pub(crate) fn is_token_head_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || byte == b'_'
        || byte == b'}'
        || byte == b'{'
        || byte == b'`'
        || byte == b'@'
        || byte == b'$'
        || byte == b'('
        || byte == b'['
        || byte == b'<'
        || is_delimiter_byte(byte)
        || is_operator_byte(byte)
}

pub(crate) fn has_suffix_ignoring_ws_before(source: &str, before: usize, suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    let bytes = source.as_bytes();
    let mut pos = before.min(source.len());
    while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    if pos < suffix.len() {
        return false;
    }
    let start = pos - suffix.len();
    &source[start..pos] == suffix
}

pub(crate) fn token_ends_like_literal(source: &str, before: usize) -> bool {
    let Some(end) = token_end_before(source, before) else {
        return false;
    };
    let bytes = source.as_bytes();
    let last = bytes[end - 1];
    if matches!(last, b'"' | b'\'') {
        return true;
    }
    if last.is_ascii_digit() || last == b'.' {
        return true;
    }
    for kw in ["true", "false"] {
        if end >= kw.len() && &source[end - kw.len()..end] == kw {
            let before_kw = end - kw.len();
            if before_kw == 0 || !is_ident_continue(bytes[before_kw - 1]) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn token_ends_like_list_element(source: &str, before: usize) -> bool {
    let Some(end) = token_end_before(source, before) else {
        return false;
    };
    let last = source.as_bytes()[end - 1];
    if is_close_delimiter_byte(last) {
        return true;
    }
    if ident_span_before(source, before).is_some() {
        return true;
    }
    token_ends_like_literal(source, before)
}

pub(crate) fn looks_like_list_peer_start(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    let b = source.as_bytes()[pos];
    matches!(b, b'(' | b'[') || is_ident_start(b)
}

pub(crate) fn token_end_before(source: &str, before: usize) -> Option<usize> {
    let mut pos = before.min(source.len());
    let bytes = source.as_bytes();
    while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    if pos == 0 {
        return None;
    }
    Some(pos)
}

pub(crate) fn prev_non_ws_byte(source: &str, before: usize) -> Option<u8> {
    let end = token_end_before(source, before)?;
    Some(source.as_bytes()[end - 1])
}

/// Identifier token classification helper for consumers that use a byte-oriented predicate.
pub(crate) fn is_ident_continue_byte(bytes: u8) -> bool {
    is_ident_continue(bytes)
}

pub(crate) fn ident_span_ending_at(source: &str, end: usize) -> Option<(usize, usize)> {
    if end == 0 {
        return None;
    }
    let bytes = source.as_bytes();
    if !is_ident_continue_byte(bytes[end - 1]) {
        return None;
    }

    let mut start = end - 1;
    while start > 0 && is_ident_continue_byte(bytes[start - 1]) {
        start -= 1;
    }

    if !is_ident_start(bytes[start]) {
        return None;
    }

    Some((start, end))
}

pub(crate) fn ident_span_before(source: &str, before: usize) -> Option<(usize, usize)> {
    let end = token_end_before(source, before)?;
    ident_span_ending_at(source, end)
}

pub(crate) fn ident_span_at(source: &str, pos: usize) -> Option<(usize, usize)> {
    let pos = skip_ws(source, pos);
    if pos >= source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    if !is_ident_start(bytes[pos]) {
        return None;
    }

    let mut end = pos + 1;
    while end < source.len() && is_ident_continue_byte(bytes[end]) {
        end += 1;
    }
    Some((pos, end))
}

/// Recognize primitive-like type starts in recovery heuristics.
pub(crate) fn looks_like_type_keyword(source: &str, pos: usize) -> bool {
    syntax_primitives::PRIMITIVE_TYPE_KEYWORDS.iter().any(|kw| keyword_at(source, pos, kw))
}

/// Recognize expression-like tokens when inferring separators.
pub(crate) fn looks_like_expression_start(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }

    let b = source.as_bytes()[pos];
    matches!(b, b'(' | b'[' | b'{' | b'<' | b'-' | b'!' | b'"' | b'\'' | b'_' | b'$')
        || b.is_ascii_digit()
        || is_ident_start(b)
}

pub(crate) fn looks_like_type_or_expression_start(source: &str, pos: usize) -> bool {
    looks_like_expression_start(source, pos) || looks_like_type_keyword(source, pos)
}

/// Fast identifier scan after a known start offset.
pub(crate) fn skip_identifier(source: &str, pos: usize) -> usize {
    let bytes = source.as_bytes();
    let mut i = pos;
    if i < source.len() && is_ident_start(bytes[i]) {
        i += 1;
        while i < source.len() && is_ident_continue(bytes[i]) {
            i += 1;
        }
    }
    i
}

pub(crate) fn skip_string_or_char(source: &str, start: usize) -> usize {
    let quote = source.as_bytes()[start];
    let mut i = start + 1;
    while i < source.len() {
        if source.as_bytes()[i] == b'\\' {
            i = (i + 2).min(source.len());
            continue;
        }
        if source.as_bytes()[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    source.len()
}

/// Duplicate of `keyword_at` logic in `utils`, kept in scan primitives for shared use.
pub(crate) fn keyword_at(source: &str, pos: usize, keyword: &str) -> bool {
    let bytes = source.as_bytes();
    if pos + keyword.len() > bytes.len() || &bytes[pos..pos + keyword.len()] != keyword.as_bytes() {
        return false;
    }
    if pos > 0 {
        let before = bytes[pos - 1];
        if is_ident_continue(before) {
            return false;
        }
    }
    let after = pos + keyword.len();
    if after < bytes.len() && is_ident_continue(bytes[after]) {
        return false;
    }
    true
}

/// First non-whitespace token start at or after `from`, preferring statement/item boundaries.
pub(crate) fn next_token_start(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pos = skip_ws(source, from);
    if pos >= source.len() {
        return None;
    }
    if is_token_head_byte(bytes[pos]) {
        return Some(pos);
    }
    while pos < source.len() {
        if is_token_head_byte(bytes[pos]) {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

pub(crate) fn token_len_at_raw(source: &str, start: usize) -> Option<usize> {
    if start >= source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let first = bytes[start];
    let end = if first == b'"' || first == b'\'' {
        let mut cursor = start + 1;
        while cursor < source.len() {
            if bytes[cursor] == b'\\' {
                cursor = (cursor + 2).min(source.len());
                continue;
            }
            if bytes[cursor] == first {
                cursor += 1;
                break;
            }
            cursor += 1;
        }
        cursor
    } else if first == b'@' && start + 1 < source.len() && bytes[start + 1] == b'{' {
        let mut cursor = start + 2;
        let mut depth = 1i32;
        while cursor < source.len() {
            if bytes[cursor] == b'\\' {
                cursor = (cursor + 2).min(source.len());
                continue;
            }

            match bytes[cursor] {
                b'"' => {
                    cursor = (cursor + 1).min(source.len());
                    while cursor < source.len() {
                        if bytes[cursor] == b'\\' {
                            cursor = (cursor + 2).min(source.len());
                            continue;
                        }
                        if bytes[cursor] == b'"' {
                            cursor += 1;
                            break;
                        }
                        cursor += 1;
                    }
                }
                b'\'' => {
                    cursor = (cursor + 1).min(source.len());
                    while cursor < source.len() {
                        if bytes[cursor] == b'\\' {
                            cursor = (cursor + 2).min(source.len());
                            continue;
                        }
                        if bytes[cursor] == b'\'' {
                            cursor += 1;
                            break;
                        }
                        cursor += 1;
                    }
                }
                b'{' => {
                    depth += 1;
                    cursor += 1;
                }
                b'}' => {
                    depth -= 1;
                    cursor += 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => cursor += 1,
            }
        }
        cursor
    } else if first == b'`' {
        let mut fence_len = 1usize;
        while start + fence_len < source.len() && source.as_bytes()[start + fence_len] == b'`' && fence_len < 3 {
            fence_len += 1;
        }
        start + fence_len
    } else if is_operator_byte(first) {
        if let Some(len) = operator_token_len(source, start) { start + len } else { start + 1 }
    } else if is_ident_start(first) {
        skip_identifier(source, start)
    } else if first.is_ascii_digit() {
        let mut cursor = start;
        while cursor < source.len() && (bytes[cursor].is_ascii_digit() || bytes[cursor] == b'.') {
            cursor += 1;
        }
        cursor
    } else if first == b'$' {
        let mut cursor = start.saturating_add(1);
        while cursor < source.len() && is_ident_continue(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == start.saturating_add(1) { start + 1 } else { cursor }
    } else {
        start + 1
    };
    Some(end.saturating_sub(start).min(source.len().saturating_sub(start)))
}

/// Net open counts for `()[]{}<>` from the start of `source` through `through` (exclusive).
pub(crate) fn unbalanced_delimiters(source: &str, through: usize) -> (i32, i32, i32, i32) {
    let through = through.min(source.len());
    let bytes = source.as_bytes();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut angle = 0i32;
    let mut i = 0usize;
    while i < through {
        // Skip string / char / line comments roughly so fence recovery stays stable.
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < through {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(through);
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            b'\'' => {
                i += 1;
                while i < through {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(through);
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < through && bytes[i + 1] == b'/' => {
                i += 2;
                while i < through && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < through && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < through && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(through);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            b'{' => brace += 1,
            b'}' => brace -= 1,
            b'<' => angle += 1,
            b'>' => angle -= 1,
            _ => {}
        }
        i += 1;
    }
    (paren, bracket, brace, angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_head_includes_interpolated_identifier_prefix() {
        assert!(is_token_head_byte(b'$'));
        assert!(is_token_head_byte(b'x'));
    }

    #[test]
    fn token_len_for_interpolated_identifier() {
        let source = "$value + 1";
        let start = source.find('$').expect("missing identifier prefix");
        assert_eq!(token_len_at_raw(source, start), Some(6));
    }

    #[test]
    fn next_token_start_finds_interpolated_identifiers() {
        let source = "let x = $value + 1";
        let error_pos = source.find('+').expect("missing operator");
        let maybe_start = next_token_start(source, error_pos + 1).expect("missing next token");
        assert_eq!(&source[maybe_start..], "1");
    }

    #[test]
    fn token_head_includes_operators() {
        let source = "a + b";
        let plus_pos = source.find('+').expect("missing plus");
        assert!(is_token_head_byte(source.as_bytes()[plus_pos]));
        assert_eq!(token_len_at_raw(source, plus_pos), Some(1));

        let op_source = "x === y << z >> w";
        let triple_pos = op_source.find("===").expect("missing ===");
        assert_eq!(token_len_at_raw(op_source, triple_pos), Some(3));
        let shift_left_pos = op_source.find("<<").expect("missing <<");
        assert_eq!(token_len_at_raw(op_source, shift_left_pos), Some(2));
        let shift_right_pos = op_source.find(">>").expect("missing >>");
        assert_eq!(token_len_at_raw(op_source, shift_right_pos), Some(2));
    }

    #[test]
    fn token_head_includes_backtick() {
        assert!(is_token_head_byte(b'`'));
        let source = "code```txt\n```";
        let fence_pos = source.find('`').expect("missing code fence");
        let next_token = next_token_start(source, fence_pos).expect("missing next token");
        assert_eq!(next_token, fence_pos);
        assert_eq!(token_len_at_raw(source, fence_pos), Some(3));
    }

    #[test]
    fn token_len_for_code_hole() {
        let source = "@{value + 1}";
        let hole_pos = source.find("@{").expect("missing code hole");
        assert_eq!(token_len_at_raw(source, hole_pos), Some(source.len() - hole_pos));
    }
}
