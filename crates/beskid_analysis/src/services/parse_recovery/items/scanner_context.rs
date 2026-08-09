use super::super::scan::skip_ws;
use super::super::{scan, syntax_primitives};

pub(super) fn near_item_body_context(source: &str, error_pos: usize) -> bool {
    let scan_through = error_pos.min(source.len());
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < scan_through {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }
        if item_body_opener_before(source, i) {
            return true;
        }
        i += 1;
    }
    false
}

fn item_body_opener_before(source: &str, brace_pos: usize) -> bool {
    let before = source[..brace_pos].trim_end();
    if before.is_empty() {
        return false;
    }

    if before.contains("extend type") {
        return true;
    }
    for keyword in syntax_primitives::ITEM_BODY_OPEN_KEYWORDS {
        if *keyword == "extend" {
            continue;
        }
        if *keyword == "host" {
            continue;
        }
        if *keyword == "macro" {
            continue;
        }
        if *keyword == "mod" {
            continue;
        }
        if matches_item_keyword_before_brace(before, keyword) {
            return true;
        }
    }

    if matches_item_keyword_before_brace(before, "host") && before.ends_with(')') {
        return true;
    }
    if matches_item_keyword_before_brace(before, "macro") && before.ends_with(')') {
        return true;
    }
    if matches_item_keyword_before_brace(before, "mod") && !before.contains('.') {
        return true;
    }
    false
}

fn matches_item_keyword_before_brace(snippet: &str, keyword: &str) -> bool {
    let mut tail_start = snippet.len().saturating_sub(256);
    while tail_start > 0 && !snippet.is_char_boundary(tail_start) {
        tail_start -= 1;
    }
    let tail = &snippet[tail_start..];
    let mut pos = 0usize;
    while pos < tail.len() {
        if scan::keyword_at(tail, pos, keyword) {
            return true;
        }
        pos += 1;
    }
    false
}

pub(super) fn inside_contract_block(source: &str, error_pos: usize) -> bool {
    let mut pos = 0usize;
    while pos < source.len() {
        if !scan::keyword_at(source, pos, "contract") {
            pos += 1;
            continue;
        }
        let open_brace = find_next_brace_after(source, pos + "contract".len());
        let Some(open_brace) = open_brace else {
            pos += 1;
            continue;
        };
        let close_brace = find_matching_close_brace(source, open_brace);
        let end = close_brace.unwrap_or(source.len());
        if error_pos > open_brace && error_pos <= end {
            return true;
        }
        pos = open_brace + 1;
    }
    false
}

fn find_next_brace_after(source: &str, from: usize) -> Option<usize> {
    let mut pos = skip_ws(source, from);
    while pos < source.len() {
        match source.as_bytes()[pos] {
            b'"' | b'\'' => pos = scan::skip_string_or_char(source, pos),
            b'/' if pos + 1 < source.len() && source.as_bytes()[pos + 1] == b'/' => {
                pos += 2;
                while pos < source.len() && source.as_bytes()[pos] != b'\n' {
                    pos += 1;
                }
            }
            b'/' if pos + 1 < source.len() && source.as_bytes()[pos + 1] == b'*' => {
                pos += 2;
                while pos + 1 < source.len() && !(source.as_bytes()[pos] == b'*' && source.as_bytes()[pos + 1] == b'/')
                {
                    pos += 1;
                }
                pos = (pos + 2).min(source.len());
            }
            b'{' => return Some(pos),
            _ => pos += 1,
        }
    }
    None
}

fn find_matching_close_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut pos = open;
    while pos < source.len() {
        match bytes[pos] {
            b'"' | b'\'' => {
                pos = scan::skip_string_or_char(source, pos);
                continue;
            }
            b'/' if pos + 1 < source.len() && bytes[pos + 1] == b'/' => {
                pos += 2;
                while pos < source.len() && bytes[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            b'/' if pos + 1 < source.len() && bytes[pos + 1] == b'*' => {
                pos += 2;
                while pos + 1 < source.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                    pos += 1;
                }
                pos = (pos + 2).min(source.len());
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

pub(super) fn find_close_paren_near(source: &str, error_pos: usize) -> Option<usize> {
    let mut pos = error_pos.min(source.len());
    let lower_bound = pos.saturating_sub(512);
    while pos > lower_bound {
        pos -= 1;
        if source.as_bytes()[pos] == b')' {
            return Some(pos);
        }
    }
    None
}

pub(super) fn looks_like_signature_close_paren(source: &str, close_paren: usize) -> bool {
    let Some(open_paren) = find_matching_open_paren(source, close_paren) else {
        return false;
    };
    let before_open = source[..open_paren].trim_end();
    if before_open.is_empty() {
        return false;
    }
    let bytes = before_open.as_bytes();
    let last = bytes[bytes.len() - 1];
    last.is_ascii_alphanumeric() || last == b'_' || last == b')' || last == b']' || last == b'>'
}

fn find_matching_open_paren(source: &str, close_paren: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut pos = close_paren;
    while pos > 0 {
        match bytes[pos] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => {}
        }
        pos -= 1;
    }
    None
}

pub(super) fn item_body_keyword_before_brace(source: &str, brace_pos: usize) -> Option<&'static str> {
    if brace_pos == 0 {
        return None;
    }

    let mut best: Option<(usize, &str)> = None;
    for (keyword, token) in [("type", "type"), ("enum", "enum")] {
        if let Some(pos) = scan::find_keyword_backward(source, brace_pos, keyword) {
            if brace_pos.saturating_sub(pos) > 384 {
                continue;
            }

            if source[pos..brace_pos].contains('{') || source[pos..brace_pos].contains("}") {
                continue;
            }

            if best.is_some_and(|(best_pos, _)| pos <= best_pos) {
                continue;
            }

            best = Some((pos, token));
        }
    }

    best.map(|(_, token)| token)
}

pub(super) fn error_near_eof(source: &str, error_pos: usize) -> bool {
    let eof = source.trim_end().len();
    skip_ws(source, error_pos) >= eof.saturating_sub(1)
}
