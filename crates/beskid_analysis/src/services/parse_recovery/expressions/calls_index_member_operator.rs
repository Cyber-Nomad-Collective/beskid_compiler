use crate::parser::Rule;

use super::super::scan::skip_ws;
use super::super::{candidate::RepairCandidate, expected_tokens, lists, scan, syntax_primitives};
use super::literals_lists::{inside_expression_argument_list, inside_grouped_expression, inside_pattern_list};
use super::priorities::{
    PRI_CALL_TRAILING_COMMA_DELETE, PRI_CALL_TRAILING_COMMA_FIX, PRI_EXPR_CONTROL_BODY, PRI_EXPR_EXPECTED_OPERATOR,
    PRI_INDEX_CLOSE, PRI_INDEX_PLACEHOLDER, PRI_MEMBER_ACCESS_STUB,
};
use super::scanner_context::prefix_before;

pub(super) fn paren_argument_separator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    lists::trailing_separator_before_close_delimiter(
        source,
        error_pos,
        insert_at,
        candidates,
        b'(',
        b')',
        |source, open, scan_pos| {
            inside_expression_argument_list(source, open, scan_pos)
                || inside_grouped_expression(source, scan_pos)
                || inside_pattern_list(source, scan_pos)
        },
        "0",
        PRI_CALL_TRAILING_COMMA_DELETE,
        PRI_CALL_TRAILING_COMMA_FIX,
        "removed trailing comma in expression argument list",
        "inserted placeholder argument after trailing comma",
    );
}

pub(super) fn expression_operator_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    parse_error: &pest::error::Error<Rule>,
    candidates: &mut Vec<RepairCandidate>,
) {
    if !expression_expected_after_operator(parse_error) {
        return;
    }

    let Some(prev) = scan::prev_non_ws_byte(source, error_pos) else {
        return;
    };
    if !scan::is_operator_byte(prev) {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        "0",
        "inserted placeholder expression after trailing operator",
        PRI_EXPR_EXPECTED_OPERATOR,
    ));
}

pub(super) fn member_access_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    _parse_error: &pest::error::Error<Rule>,
    candidates: &mut Vec<RepairCandidate>,
) {
    let tail_pos = source.trim_end().len();
    let dot_pos = if error_pos > 0 && error_pos <= source.len() && source.as_bytes()[error_pos - 1] == b'.' {
        error_pos - 1
    } else if error_pos >= tail_pos && tail_pos > 0 && source.as_bytes()[tail_pos.saturating_sub(1)] == b'.' {
        tail_pos.saturating_sub(1)
    } else {
        return;
    };

    let Some(prev) = scan::prev_non_ws_byte(source, dot_pos.saturating_add(1)) else {
        return;
    };
    if prev != b'.' {
        return;
    }

    if !dot_access_prefix_looks_expression(source, dot_pos) {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        "field",
        "inserted placeholder member access field",
        PRI_MEMBER_ACCESS_STUB,
    ));
}

pub(super) fn index_expression_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    candidates: &mut Vec<RepairCandidate>,
) {
    let tail_pos = source.trim_end().len();
    let seek_pos = if error_pos >= tail_pos && source[..tail_pos].ends_with('[') { tail_pos } else { error_pos };

    let Some(bracket_open) = find_unclosed_bracket_before(source, seek_pos.saturating_add(1)) else {
        return;
    };

    if !index_open_is_expression_context(source, bracket_open) {
        return;
    }

    let scan_pos = skip_ws(source, bracket_open + 1);
    if scan_pos > source.len() {
        return;
    }
    let scan_end = seek_pos.min(source.len());
    let inside = if scan_end <= scan_pos { "" } else { source[scan_pos..scan_end].trim() };
    if inside.is_empty() {
        candidates.push(RepairCandidate::insert(
            insert_at,
            "0]",
            "inserted index placeholder and closed bracket",
            PRI_INDEX_PLACEHOLDER,
        ));
    } else {
        candidates.push(RepairCandidate::insert(insert_at, "]", "closed incomplete index expression", PRI_INDEX_CLOSE));
    }
}

pub(super) fn control_expression_body_repairs(
    source: &str,
    error_pos: usize,
    insert_at: usize,
    parse_error: &pest::error::Error<Rule>,
    candidates: &mut Vec<RepairCandidate>,
) {
    if !syntax_primitives::recovery_expected_or_follow_token_has_body_hint(parse_error)
        && !syntax_primitives::recovery_source_has_fallback_control_flow_hint(
            source,
            error_pos,
            syntax_primitives::CONTROL_EXPRESSION_KEYWORDS,
        )
    {
        return;
    }

    let mut latest = None::<(usize, &str)>;

    for &keyword in syntax_primitives::CONTROL_EXPRESSION_KEYWORDS {
        if let Some(pos) = scan::find_keyword_backward(source, error_pos, keyword) {
            latest = match latest {
                Some((existing_pos, existing_keyword)) if existing_pos > pos => Some((existing_pos, existing_keyword)),
                _ => Some((pos, keyword)),
            };
        }
    }

    let Some((kw_pos, keyword)) = latest else {
        return;
    };

    let prefix = prefix_before(source, kw_pos);
    if prefix.ends_with('.') {
        return;
    }

    if kw_pos + keyword.len() >= source.len() || kw_pos + keyword.len() >= error_pos {
        return;
    }

    let after_keyword = skip_ws(source, kw_pos + keyword.len());
    if source[after_keyword..error_pos].contains('{') {
        return;
    }

    let near_tail = source[after_keyword..error_pos].trim_end();
    if near_tail.is_empty() {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        " { }",
        "inserted placeholder control-expression body",
        PRI_EXPR_CONTROL_BODY,
    ));
}

fn expression_expected_after_operator(parse_error: &pest::error::Error<Rule>) -> bool {
    syntax_primitives::recovery_expected_token_has_any_class(
        parse_error,
        &[
            expected_tokens::ReplacementTokenClass::Identifier,
            expected_tokens::ReplacementTokenClass::Number,
            expected_tokens::ReplacementTokenClass::StringLike,
            expected_tokens::ReplacementTokenClass::Keyword,
            expected_tokens::ReplacementTokenClass::Delimiter,
        ],
    )
}

fn dot_access_prefix_looks_expression(source: &str, dot_pos: usize) -> bool {
    if dot_pos == 0 {
        return false;
    }

    let bytes = source.as_bytes();
    let mut pos = dot_pos;
    pos = pos.saturating_sub(1);
    while pos > 0 && bytes[pos].is_ascii_whitespace() {
        pos -= 1;
    }

    let Some(prev) = bytes.get(pos).copied() else {
        return false;
    };
    if prev == b'.' || prev == b':' || prev == b'(' || prev == b',' {
        return false;
    }

    scan::is_ident_start(prev) || prev == b')' || prev == b']' || prev == b'}' || prev == b'"' || prev == b'\''
}

fn find_unclosed_bracket_before(source: &str, error_pos: usize) -> Option<usize> {
    syntax_primitives::find_unclosed_delimiter_before(source, error_pos, b'[', b']')
}

fn index_open_is_expression_context(source: &str, open: usize) -> bool {
    let Some(mut pos) = open.checked_sub(1) else {
        return false;
    };

    while pos > 0 {
        let b = source.as_bytes()[pos];
        if b.is_ascii_whitespace() {
            pos -= 1;
            continue;
        }
        return scan::is_ident_continue(b) || matches!(b, b')' | b']' | b'}') || b.is_ascii_digit();
    }

    false
}
