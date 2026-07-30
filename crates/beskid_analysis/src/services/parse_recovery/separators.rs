//! Separator-oriented parse recovery (`;`, `,`, `:`, `=>`, `::`, `.`).

use crate::parser::Rule;

use super::{candidate::RepairCandidate, scan::{self, next_token_start, skip_ws}, syntax_primitives};

/// Generate separator insertion repairs near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, _parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let bytes = source.as_bytes();
    let scan_pos = syntax_primitives::recovery_scan_pos(source, error_pos);

    // --- Semicolon (priorities 20, 21, 30) ---

    let maybe_error_boundary = skip_ws(source, scan_pos);
    if maybe_error_boundary < source.len() && bytes[maybe_error_boundary] == b'}' {
        candidates.push(RepairCandidate::insert(
            maybe_error_boundary,
            ";",
            "inserted semicolon before block close to satisfy parser boundary",
            20,
        ));
    }

    if let Some(next_token_pos) = next_token_start(source, scan_pos) {
        if next_token_pos > 0 {
            candidates.push(RepairCandidate::insert(next_token_pos, ";", "inserted semicolon before next token", 21));
        }

        comma_before_peer(source, next_token_pos, &mut candidates);
        colon_after_identifier(source, next_token_pos, &mut candidates);
        fat_arrow_before_body(source, next_token_pos, &mut candidates);
    }

    double_colon_enum_path(source, scan_pos, &mut candidates);
    dot_member_access(source, scan_pos, &mut candidates);

    let trimmed_len = source.trim_end().len();
    if trimmed_len > 0 && !source[..trimmed_len].ends_with(';') {
        candidates.push(RepairCandidate::insert(trimmed_len, ";", "inserted missing semicolon at end of file", 30));
    }

    candidates
}

fn comma_before_peer(source: &str, next_token_pos: usize, out: &mut Vec<RepairCandidate>) {
    if !scan::looks_like_list_peer_start(source, next_token_pos) {
        return;
    }
    if !scan::token_ends_like_list_element(source, next_token_pos) {
        return;
    }
    out.push(RepairCandidate::insert(next_token_pos, ",", "inserted comma before next list element", 22));
}

fn colon_after_identifier(source: &str, next_token_pos: usize, out: &mut Vec<RepairCandidate>) {
    let Some((ident_start, ident_end)) = scan::ident_span_before(source, next_token_pos) else {
        return;
    };
    if ident_start == ident_end {
        return;
    }
    let gap = skip_ws(source, ident_end);
    if gap < next_token_pos && source[gap..next_token_pos].contains(':') {
        return;
    }
    if !scan::looks_like_type_or_expression_start(source, next_token_pos) {
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
    if scan::has_suffix_ignoring_ws_before(source, next_token_pos, "=>") {
        return;
    }
    if !scan::looks_like_expression_start(source, next_token_pos) {
        return;
    }

    let prev_non_ws = scan::prev_non_ws_byte(source, next_token_pos);
    let patternish = matches!(prev_non_ws, Some(b')' | b'_'))
        || scan::ident_span_before(source, next_token_pos).is_some()
        || scan::token_ends_like_literal(source, next_token_pos);

    if !patternish {
        return;
    }

    out.push(RepairCandidate::insert(next_token_pos, "=>", "inserted fat arrow before expression body", 28));
}

fn double_colon_enum_path(source: &str, error_pos: usize, out: &mut Vec<RepairCandidate>) {
    let Some(probe) = next_token_start(source, error_pos).or_else(|| (error_pos < source.len()).then_some(error_pos)) else {
        return;
    };
    let Some((_, prev_end)) = scan::ident_span_before(source, probe) else {
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
    if scan::ident_span_at(source, next).is_none() {
        return;
    }
    out.push(RepairCandidate::insert(gap, "::", "inserted path separator between enum path segments", 31));
}

fn dot_member_access(source: &str, error_pos: usize, out: &mut Vec<RepairCandidate>) {
    let boundary = skip_ws(source, error_pos);
    let prev_non_ws = scan::prev_non_ws_byte(source, boundary);
    let prev_ok = matches!(prev_non_ws, Some(b')')) || scan::ident_span_before(source, boundary).is_some();
    if !prev_ok {
        return;
    }

    let prev_end = scan::token_end_before(source, boundary).unwrap_or(boundary);
    let gap = skip_ws(source, prev_end);
    if gap < source.len() && source.as_bytes()[gap] == b'.' {
        return;
    }

    let next = skip_ws(source, boundary.max(gap));
    if scan::ident_span_at(source, next).is_none() {
        return;
    }

    out.push(RepairCandidate::insert(gap, ".", "inserted member access dot between path segments", 33));
}
