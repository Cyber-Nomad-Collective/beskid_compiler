//! Shared keyword scanning, identifier parsing, and string-literal helpers
//! used across parse-recovery submodules (items, separators, expressions, delimiters).

/// Keyword boundary check: returns true when `source[pos..]` starts with `keyword`
/// and the keyword is surrounded by non-identifier characters or boundaries.
pub(crate) fn keyword_at(source: &str, pos: usize, keyword: &str) -> bool {
    let bytes = source.as_bytes();
    if bytes.len() < pos + keyword.len() || &bytes[pos..pos + keyword.len()] != keyword.as_bytes() {
        return false;
    }
    if pos > 0 {
        let before = bytes[pos - 1];
        if before.is_ascii_alphanumeric() || before == b'_' {
            return false;
        }
    }
    let after = pos + keyword.len();
    if after < bytes.len() {
        let next = bytes[after];
        if next.is_ascii_alphanumeric() || next == b'_' {
            return false;
        }
    }
    true
}

/// Skip past a string or char literal starting with `'` or `"` at `start`.
/// Returns the byte-index after the closing delim or `source.len()` if unclosed.
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

/// Whitespace-skipping token-end before `before` (exclusive).
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

/// Last non-whitespace byte before `before` (exclusive).
pub(crate) fn prev_non_ws_byte(source: &str, before: usize) -> Option<u8> {
    let end = token_end_before(source, before)?;
    Some(source.as_bytes()[end - 1])
}

/// Identifier span (`start..end`) ending at `end` (exclusive).
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
    if !is_ident_start(bytes[start]) && bytes[start] != b'_' {
        return None;
    }
    Some((start, end))
}

/// Identifier span (`start..end`) before `before` (exclusive).
pub(crate) fn ident_span_before(source: &str, before: usize) -> Option<(usize, usize)> {
    let end = token_end_before(source, before)?;
    ident_span_ending_at(source, end)
}

/// Identifier span `(start, end)` starting at `pos` (after whitespace).
pub(crate) fn ident_span_at(source: &str, pos: usize) -> Option<(usize, usize)> {
    let pos = crate::services::parse_recovery::skip_ws(source, pos);
    if pos >= source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    if bytes[pos] == b'_' {
        let end = if pos + 1 < source.len() && is_ident_continue_byte(bytes[pos + 1]) {
            let mut end = pos + 2;
            while end < source.len() && is_ident_continue_byte(bytes[end]) {
                end += 1;
            }
            end
        } else {
            pos + 1
        };
        return Some((pos, end));
    }
    if !is_ident_start(bytes[pos]) {
        return None;
    }
    let mut end = pos + 1;
    while end < source.len() && is_ident_continue_byte(bytes[end]) {
        end += 1;
    }
    Some((pos, end))
}

pub(crate) fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

pub(crate) fn is_ident_continue_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(crate) fn has_ident_continue_at(bytes: &[u8], pos: usize) -> bool {
    pos < bytes.len() && is_ident_continue_byte(bytes[pos])
}
