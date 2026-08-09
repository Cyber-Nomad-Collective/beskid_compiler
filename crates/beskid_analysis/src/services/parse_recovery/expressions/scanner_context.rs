use super::super::scan::{next_token_start, skip_ws};
use super::super::{scan, syntax_primitives};

pub(super) fn recovery_insert_pos(source: &str, error_pos: usize) -> usize {
    next_token_start(source, error_pos).unwrap_or_else(|| source.trim_end().len())
}

pub(super) fn find_match_block_brace(source: &str, through: usize) -> Option<usize> {
    let match_kw = scan::find_keyword_backward(source, through, "match")?;
    let after_kw = skip_ws(source, match_kw + "match".len());
    let brace = find_next_brace_after_expression(source, after_kw, through)?;
    if through > brace { Some(brace) } else { None }
}

fn find_next_brace_after_expression(source: &str, from: usize, limit: usize) -> Option<usize> {
    let limit = limit.min(source.len());
    let mut pos = from;
    while pos < limit {
        pos = skip_ws(source, pos);
        if pos >= limit {
            break;
        }
        match source.as_bytes()[pos] {
            b'{' => return Some(pos),
            b'(' | b'[' | b'"' | b'\'' => {
                pos = skip_balanced_token(source, pos, limit)?;
            }
            _ => {
                if scan::is_ident_start(source.as_bytes()[pos]) {
                    pos = scan::skip_identifier(source, pos);
                } else {
                    pos += 1;
                }
            }
        }
    }
    None
}

fn skip_balanced_token(source: &str, pos: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if pos >= limit {
        return None;
    }
    match bytes[pos] {
        b'(' => skip_balanced_delim(source, pos, limit, b'(', b')'),
        b'[' => skip_balanced_delim(source, pos, limit, b'[', b']'),
        b'"' => skip_string(source, pos, limit),
        b'\'' => skip_char(source, pos, limit),
        _ => Some(pos + 1),
    }
}

fn skip_balanced_delim(source: &str, open_pos: usize, limit: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = open_pos;
    while i < limit {
        match bytes[i] {
            b'"' => i = skip_string(source, i, limit)?,
            b'\'' => i = skip_char(source, i, limit)?,
            b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                i += 2;
                while i < limit && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(limit);
            }
            c if c == open => {
                depth += 1;
                i += 1;
            }
            c if c == close => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => i += 1,
        }
    }
    None
}

fn skip_string(source: &str, pos: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = pos + 1;
    while i < limit {
        if bytes[i] == b'\\' {
            i = (i + 2).min(limit);
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i + 1);
        }
        i += 1;
    }
    Some(limit)
}

fn skip_char(source: &str, pos: usize, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = pos + 1;
    while i < limit {
        if bytes[i] == b'\\' {
            i = (i + 2).min(limit);
            continue;
        }
        if bytes[i] == b'\'' {
            return Some(i + 1);
        }
        i += 1;
    }
    Some(limit)
}

pub(super) fn find_unclosed_paren_before(source: &str, error_pos: usize) -> Option<usize> {
    syntax_primitives::find_unclosed_delimiter_before(source, error_pos, b'(', b')')
}

pub(super) fn prefix_before(source: &str, pos: usize) -> &str {
    let end = skip_ws(source, pos);
    let mut start = end;
    while start > 0 {
        start -= 1;
        let b = source.as_bytes()[start];
        if b.is_ascii_whitespace() {
            break;
        }
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'.' || b == b'!') {
            break;
        }
    }
    source[start..end].trim()
}
