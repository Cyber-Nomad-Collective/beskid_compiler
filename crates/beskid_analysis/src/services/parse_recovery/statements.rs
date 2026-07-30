//! Statement-oriented recovery primitives (`if/while` bodies, terminator statements).

use crate::parser::Rule;

use super::scan::{next_token_start, skip_ws};
use super::{candidate::RepairCandidate, scan, syntax_primitives};

const PRI_CONTROL_FLOW_BODY: u8 = 74;
const PRI_STATEMENT_TERMINATOR: u8 = 75;
const PRI_PROGRAM_ROOT_TERMINATOR: u8 = 76;

/// Generate statement-oriented repairs for incomplete control flow and terminated statements.
pub fn repairs(source: &str, error_pos: usize, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let insert_at = recovery_insert_pos(source, error_pos);
    let statement_keywords = statement_recovery_keywords(parse_error);
    let statement_starts = syntax_primitives::top_level_statement_starts(source, 0, &statement_keywords);
    let mut candidates = Vec::new();
    control_flow_body_repairs(source, error_pos, insert_at, parse_error, &statement_starts, &mut candidates);
    statement_terminator_repairs(source, error_pos, insert_at, &statement_starts, &mut candidates);
    candidates
}

fn control_flow_body_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    parse_error: &pest::error::Error<Rule>,
    _statement_starts: &[usize],
    candidates: &mut Vec<RepairCandidate>,
) {
    let Some(kw_pos) = find_control_flow_keyword(source, error_pos) else {
        return;
    };

    if !control_flow_boundary(source, error_pos) && error_pos != 0 {
        return;
    }

    if !syntax_primitives::recovery_expected_or_follow_token_has_body_hint(parse_error)
        && !syntax_primitives::recovery_source_has_fallback_control_flow_hint(
            source,
            error_pos,
            syntax_primitives::CONTROL_FLOW_KEYWORDS,
        )
    {
        return;
    }

    let Some(keyword_len) = syntax_primitives::control_flow_keyword_len(source, kw_pos) else {
        return;
    };
    let after_kw = skip_ws(source, kw_pos + keyword_len);
    let tail_end = error_pos;
    let tail = &source[after_kw..tail_end];

    if tail.contains('{') {
        return;
    }
    if tail.ends_with('=') || tail.ends_with("=>") || tail.ends_with(':') {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        " { }",
        "inserted control-flow body placeholder",
        PRI_CONTROL_FLOW_BODY,
    ));
}

fn statement_terminator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    statement_starts: &[usize],
    candidates: &mut Vec<RepairCandidate>,
) {
    let error_pos = error_pos.min(source.len());
    for &start in statement_starts {
        if start == 0 || start > error_pos {
            continue;
        }
        if start >= source.len() {
            continue;
        }
        let boundary_insert_at = syntax_primitives::recovery_insert_position(source, start);
        let prev = boundary_insert_at.saturating_sub(1);
        if prev < source.len() && (source.as_bytes()[prev] == b';' || source.as_bytes()[prev] == b'}') {
            continue;
        }
        candidates.push(RepairCandidate::insert(
            boundary_insert_at,
            ";",
            "inserted statement terminator",
            PRI_STATEMENT_TERMINATOR,
        ));
    }

    if source.trim_end().is_empty() {
        program_root_statement_terminator_repairs(source, statement_starts, candidates);
        return;
    }

    let Some((kw_pos, keyword)) = nearest_recent_keyword(source, error_pos, syntax_primitives::TERMINATOR_KEYWORDS)
    else {
        return;
    };
    if kw_pos + keyword.len() >= source.len() {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ";",
            "inserted statement terminator",
            PRI_STATEMENT_TERMINATOR,
        ));
        return;
    }

    let after_kw = skip_ws(source, kw_pos + keyword.len());
    if after_kw >= source.len() {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ";",
            "inserted statement terminator",
            PRI_STATEMENT_TERMINATOR,
        ));
        return;
    }
    if source.as_bytes()[after_kw] == b';' || source.as_bytes()[after_kw] == b'}' {
        return;
    }
    if !statement_boundary_below(source, error_pos, after_kw) {
        return;
    }

    let (end_of_statement, at_error) = statement_truncation_range(source, kw_pos, after_kw, error_pos);
    if !at_error {
        return;
    }

    candidates.push(RepairCandidate::insert(
        end_of_statement,
        ";",
        "inserted statement terminator",
        PRI_STATEMENT_TERMINATOR,
    ));
}

fn program_root_statement_terminator_repairs(
    source: &str,
    statement_starts: &[usize],
    candidates: &mut Vec<RepairCandidate>,
) {
    if statement_starts.is_empty() {
        return;
    }

    let end_of_source = source.trim_end().len();
    if end_of_source == 0 {
        return;
    }

    if control_flow_without_body(source, statement_starts[0], source.len()) {
        return;
    }

    let boundary = if statement_starts.len() >= 2 {
        syntax_primitives::recovery_insert_position(source, statement_starts[1])
    } else {
        end_of_source
    };
    if boundary == 0 || boundary > source.len() {
        return;
    }

    let prefix = source[..boundary].trim_end();
    if prefix.is_empty()
        || prefix.ends_with(';')
        || prefix.ends_with('{')
        || prefix.ends_with('}')
        || prefix.ends_with('=')
        || prefix.ends_with(":")
        || prefix.ends_with(">")
        || prefix.ends_with(":=")
        || prefix.ends_with("=>")
        || prefix.ends_with(',')
    {
        return;
    }

    if prefix.ends_with('(') || prefix.ends_with('[') || prefix.ends_with('<') {
        return;
    }

    candidates.push(RepairCandidate::insert(
        boundary,
        ";",
        "inserted statement terminator before next top-level syntax",
        PRI_PROGRAM_ROOT_TERMINATOR,
    ));
}

fn statement_recovery_keywords(parse_error: &pest::error::Error<Rule>) -> Vec<&'static str> {
    syntax_primitives::recovery_sync_keywords(parse_error)
}

fn control_flow_without_body(source: &str, kw_pos: usize, scan_to: usize) -> bool {
    let Some(keyword_len) = control_flow_keyword_len(source, kw_pos) else {
        return false;
    };
    let after_kw = skip_ws(source, kw_pos + keyword_len);
    let tail = source[after_kw..scan_to].trim_end();
    if tail.is_empty() {
        return true;
    }

    !tail.contains('{')
        && !tail.ends_with('=')
        && !tail.ends_with(":=")
        && !tail.ends_with("=>")
        && !tail.ends_with(':')
}

fn control_flow_keyword_len(source: &str, kw_pos: usize) -> Option<usize> {
    syntax_primitives::control_flow_keyword_len(source, kw_pos)
}

fn find_control_flow_keyword(source: &str, error_pos: usize) -> Option<usize> {
    if error_pos <= 1 {
        return find_keyword_at_prefix(source);
    }

    for keyword in syntax_primitives::CONTROL_FLOW_KEYWORDS {
        if let Some(kw_pos) = find_recent_keyword_before(source, error_pos, keyword) {
            return Some(kw_pos);
        }
    }
    None
}

fn find_keyword_at_prefix(source: &str) -> Option<usize> {
    let mut start = 0usize;
    while start < source.len() && source.as_bytes()[start].is_ascii_whitespace() {
        start += 1;
    }
    if source[start..].is_empty() {
        return None;
    }
    for keyword in syntax_primitives::CONTROL_FLOW_KEYWORDS {
        if source.len() >= start + keyword.len()
            && &source[start..start + keyword.len()] == *keyword
            && source.as_bytes().get(start + keyword.len()).is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
            && (start == 0
                || !source.as_bytes()[start - 1].is_ascii_alphanumeric() && source.as_bytes()[start - 1] != b'_')
        {
            return Some(start);
        }
    }
    None
}

fn control_flow_boundary(source: &str, error_pos: usize) -> bool {
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let trimmed_pos = source.trim_end().len();
    if error_pos >= trimmed_pos && trimmed_pos > 0 {
        return true;
    }
    if error_pos == 0 {
        return looks_like_leading_control_flow_prefix(source);
    }
    let bytes = source.as_bytes();
    let before = error_pos.saturating_sub(1);
    if !bytes.get(before).is_some_and(|b| *b != b';' && *b != b'}') {
        return false;
    }
    true
}

fn looks_like_leading_control_flow_prefix(source: &str) -> bool {
    let start = skip_ws(source, 0);
    if start >= source.len() {
        return false;
    }
    for keyword in syntax_primitives::CONTROL_FLOW_KEYWORDS {
        if source.len() >= start + keyword.len()
            && &source[start..start + keyword.len()] == *keyword
            && source.as_bytes().get(start + keyword.len()).is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
        {
            return true;
        }
    }
    false
}

fn statement_boundary_below(source: &str, error_pos: usize, after_kw: usize) -> bool {
    let segment = &source[after_kw..error_pos.min(source.len())].trim();
    !segment.is_empty()
}

fn statement_truncation_range(source: &str, kw_pos: usize, after_kw: usize, error_pos: usize) -> (usize, bool) {
    let mut end = next_token_start(source, error_pos).unwrap_or_else(|| source.trim_end().len());
    if end <= kw_pos {
        end = after_kw;
    }
    (end, end >= after_kw && end <= error_pos)
}

fn nearest_recent_keyword<'a>(source: &'a str, error_pos: usize, keywords: &'a [&'a str]) -> Option<(usize, &'a str)> {
    let mut found: Option<(usize, &'a str)> = None;
    let limit = error_pos.min(source.len());
    for &keyword in keywords {
        let Some(pos) = find_recent_keyword_before(source, limit, keyword) else {
            continue;
        };
        if found.is_none_or(|(current, _)| pos > current) {
            found = Some((pos, keyword));
        }
    }
    found
}

fn find_recent_keyword_before(source: &str, through: usize, keyword: &str) -> Option<usize> {
    scan::find_keyword_backward(source, through, keyword)
}

fn recovery_insert_pos(source: &str, error_pos: usize) -> usize {
    next_token_start(source, error_pos).unwrap_or_else(|| source.trim_end().len())
}
