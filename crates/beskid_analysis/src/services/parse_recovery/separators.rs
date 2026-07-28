//! Separator-oriented parse recovery (`;`, `,`, `:`, `=>`, `::`, `.`).

use crate::parser::Rule;

use super::{
    RepairCandidate, next_token_start, skip_ws,
    utils::{
        has_ident_continue_at, ident_span_at, ident_span_before, is_ident_continue_byte,
        is_ident_start, prev_non_ws_byte, token_end_before,
    },
};

/// Generate separator insertion repairs near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, _parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let bytes = source.as_bytes();

    // --- Semicolon (priorities 20, 21, 30) ---

    let maybe_error_boundary = skip_ws(source, error_pos);
    if maybe_error_boundary < source.len() && bytes[maybe_error_boundary] == b'}' {
        candidates.push(RepairCandidate::insert(
            maybe_error_boundary,
            ";",
            "inserted semicolon before block close to satisfy parser boundary",
            20,
        ));
    }

    if let Some(next_token_pos) = next_token_start(source, error_pos) {
        if next_token_pos > 0 {
            candidates.push(RepairCandidate::insert(next_token_pos, ";", "inserted semicolon before next token", 21));
        }

        comma_before_peer(source, next_token_pos, &mut candidates);
        colon_after_identifier(source, next_token_pos, &mut candidates);
        fat_arrow_before_body(source, next_token_pos, &mut candidates);
    }

    double_colon_enum_path(source, error_pos, &mut candidates);
    dot_member_access(source, error_pos, &mut candidates);

    let trimmed_len = source.trim_end().len();
    if trimmed_len > 0 && !source[..trimmed_len].ends_with(';') {
        candidates.push(RepairCandidate::insert(trimmed_len, ";", "inserted missing semicolon at end of file", 30));
    }

    candidates
}

fn comma_before_peer(source: &str, next_token_pos: usize, out: &mut Vec<RepairCandidate>) {
    if !looks_like_list_peer_start(source, next_token_pos) {
        return;
    }
    if !prev_token_looks_like_list_element_end(source, next_token_pos) {
        return;
    }
    out.push(RepairCandidate::insert(next_token_pos, ",", "inserted comma before next list element", 22));
}

fn colon_after_identifier(source: &str, next_token_pos: usize, out: &mut Vec<RepairCandidate>) {
    let Some((ident_start, ident_end)) = ident_span_before(source, next_token_pos) else {
        return;
    };
    if ident_start == ident_end {
        return;
    }
    let gap = skip_ws(source, ident_end);
    if gap < next_token_pos && source[gap..next_token_pos].contains(':') {
        return;
    }
    if !looks_like_type_or_expression_start(source, next_token_pos) {
        return;
    }
    if matches!(source.as_bytes().get(next_token_pos), Some(b'=' | b',' | b';' | b')' | b']' | b'}')) {
        return;
    }
    out.push(RepairCandidate::insert(
        next_token_pos,
        ":",
        "inserted colon between identifier and type or expression",
        25,
    ));
}

fn fat_arrow_before_body(source: &str, next_token_pos: usize, out: &mut Vec<RepairCandidate>) {
    if already_has_fat_arrow_before(source, next_token_pos) {
        return;
    }
    if !looks_like_expression_start(source, next_token_pos) {
        return;
    }

    let prev_non_ws = prev_non_ws_byte(source, next_token_pos);
    let patternish = matches!(prev_non_ws, Some(b')' | b'_'))
        || prev_token_is_identifier(source, next_token_pos)
        || prev_token_looks_like_literal(source, next_token_pos);

    if !patternish {
        return;
    }

    out.push(RepairCandidate::insert(next_token_pos, "=>", "inserted fat arrow before expression body", 28));
}

fn double_colon_enum_path(source: &str, error_pos: usize, out: &mut Vec<RepairCandidate>) {
    let probe = next_token_start(source, error_pos).unwrap_or(error_pos);
    let Some((_, prev_end)) = ident_span_before(source, probe) else {
        return;
    };
    let gap = skip_ws(source, prev_end);
    if gap < source.len() && source.is_char_boundary(gap) {
        let slice = &source[gap..];
        if slice.starts_with("::") || slice.starts_with(':') {
            return;
        }
    }
    let next = skip_ws(source, error_pos.max(gap));
    if ident_span_at(source, next).is_none() {
        return;
    }
    out.push(RepairCandidate::insert(gap, "::", "inserted path separator between enum path segments", 31));
}

fn dot_member_access(source: &str, error_pos: usize, out: &mut Vec<RepairCandidate>) {
    let boundary = skip_ws(source, error_pos);
    let prev_non_ws = prev_non_ws_byte(source, boundary);
    let prev_ok = matches!(prev_non_ws, Some(b')')) || prev_token_is_identifier(source, boundary);
    if !prev_ok {
        return;
    }

    let prev_end = token_end_before(source, boundary).unwrap_or(boundary);
    let gap = skip_ws(source, prev_end);
    if gap < source.len() && source.as_bytes()[gap] == b'.' {
        return;
    }

    let next = skip_ws(source, boundary.max(gap));
    if ident_span_at(source, next).is_none() {
        return;
    }

    out.push(RepairCandidate::insert(gap, ".", "inserted member access dot between path segments", 33));
}

fn prev_token_looks_like_list_element_end(source: &str, before: usize) -> bool {
    let Some(end) = token_end_before(source, before) else {
        return false;
    };
    let last = source.as_bytes()[end - 1];
    if matches!(last, b')' | b']' | b'}') {
        return true;
    }
    if ident_span_before(source, before).is_some() {
        return true;
    }
    prev_token_looks_like_literal(source, before)
}

fn looks_like_list_peer_start(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    let b = source.as_bytes()[pos];
    matches!(b, b'_' | b'(' | b'[') || is_ident_start(b)
}

fn looks_like_type_or_expression_start(source: &str, pos: usize) -> bool {
    looks_like_expression_start(source, pos) || looks_like_type_keyword(source, pos)
}

fn looks_like_type_keyword(source: &str, pos: usize) -> bool {
    const KEYWORDS: &[&str] =
        &["bool", "i32", "i64", "u8", "pointer", "word", "f64", "char", "string", "unit", "never"];
    KEYWORDS.iter().any(|kw| {
        source.as_bytes().get(pos..pos + kw.len()) == Some(kw.as_bytes())
            && !has_ident_continue_at(source.as_bytes(), pos + kw.len())
    })
}

fn looks_like_expression_start(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    let b = source.as_bytes()[pos];
    matches!(b, b'(' | b'[' | b'{' | b'<' | b'-' | b'!' | b'"' | b'\'' | b'_' | b'@')
        || b.is_ascii_digit()
        || is_ident_start(b)
}

fn prev_token_is_identifier(source: &str, before: usize) -> bool {
    ident_span_before(source, before).is_some()
}

fn prev_token_looks_like_literal(source: &str, before: usize) -> bool {
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
            if before_kw == 0 || !is_ident_continue_byte(bytes[before_kw - 1]) {
                return true;
            }
        }
    }
    false
}

fn already_has_fat_arrow_before(source: &str, before: usize) -> bool {
    let mut pos = before.min(source.len());
    while pos > 0 && source.as_bytes()[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    pos >= 2 && &source[pos - 2..pos] == "=>"
}
