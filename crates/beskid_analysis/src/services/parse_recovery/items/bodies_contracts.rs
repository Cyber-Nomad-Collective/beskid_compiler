use crate::parser::Rule;

use super::super::{
    candidate::RepairCandidate,
    scan::{self, skip_ws, unbalanced_delimiters},
    syntax_primitives,
};
use super::priorities::{
    PRIORITY_CONTRACT_METHOD_SEMI, PRIORITY_EMPTY_BODY_STUB, PRIORITY_FN_BODY_STUB, PRIORITY_ITEM_BRACE_EOF,
    PRIORITY_ITEM_BRACE_ERROR,
};
use super::scanner_context::{
    error_near_eof, find_close_paren_near, inside_contract_block, looks_like_signature_close_paren,
    near_item_body_context,
};

pub(super) fn unclosed_item_brace_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
    let (_, _, brace, _) = unbalanced_delimiters(source, error_pos);
    if brace <= 0 || !near_item_body_context(source, error_pos) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let eof = source.trim_end().len();
    out.push(RepairCandidate::insert(
        eof,
        "}",
        "inserted closing brace for incomplete item body at end of file",
        PRIORITY_ITEM_BRACE_EOF,
    ));

    let at_error = skip_ws(source, error_pos);
    if at_error != eof {
        out.push(RepairCandidate::insert(
            at_error,
            "}",
            "inserted closing brace for incomplete item body at error boundary",
            PRIORITY_ITEM_BRACE_ERROR,
        ));
    }
    out
}

pub(super) fn contract_method_semicolon_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
    if !inside_contract_block(source, error_pos) {
        return Vec::new();
    }
    let Some(close_paren) = find_close_paren_near(source, error_pos) else {
        return Vec::new();
    };
    let after_paren = skip_ws(source, close_paren + 1);
    if after_paren < source.len() && source.as_bytes()[after_paren] == b';' {
        return Vec::new();
    }
    if !looks_like_signature_close_paren(source, close_paren) {
        return Vec::new();
    }

    vec![RepairCandidate::insert(
        after_paren,
        ";",
        "inserted semicolon after contract method signature",
        PRIORITY_CONTRACT_METHOD_SEMI,
    )]
}

pub(super) fn missing_function_body_repairs(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    if inside_contract_block(source, error_pos) {
        return Vec::new();
    }
    let Some(close_paren) = find_close_paren_near(source, error_pos) else {
        return Vec::new();
    };
    if !looks_like_signature_close_paren(source, close_paren) {
        return Vec::new();
    }

    if !syntax_primitives::recovery_expected_or_follow_token_has_body_hint(parse_error) {
        return Vec::new();
    }

    let after_paren = skip_ws(source, close_paren + 1);
    let bytes = source.as_bytes();
    if after_paren < source.len() {
        if bytes[after_paren] == b'{' {
            return Vec::new();
        }
        if bytes[after_paren] == b'=' {
            let after_eq = skip_ws(source, after_paren + 1);
            if after_eq < source.len() && bytes[after_eq] == b'>' {
                return Vec::new();
            }
        }
    }

    vec![RepairCandidate::insert(
        after_paren,
        "{}",
        "inserted empty block body for incomplete function or method",
        PRIORITY_FN_BODY_STUB,
    )]
}

pub(super) fn empty_stub_body_repairs(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    if !syntax_primitives::recovery_expected_or_follow_token_has_body_hint(parse_error) {
        return Vec::new();
    }

    if !error_near_eof(source, error_pos) {
        return Vec::new();
    }
    let insert_pos = source.trim_end().len();
    if insert_pos == 0 {
        return Vec::new();
    }
    let trimmed = source[..insert_pos].trim_end();
    if trimmed.ends_with('{') || trimmed.ends_with('}') {
        return Vec::new();
    }

    let stub_kind = empty_stub_kind(trimmed);
    let Some(reason) = stub_kind else {
        return Vec::new();
    };

    vec![RepairCandidate::insert(insert_pos, " { }", reason, PRIORITY_EMPTY_BODY_STUB)]
}

fn empty_stub_kind(trimmed: &str) -> Option<&'static str> {
    if stub_has_keyword_ident(trimmed, "type") && !trimmed.contains('{') {
        return Some("inserted empty type body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "enum") {
        return Some("inserted empty enum body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "impl") {
        return Some("inserted empty impl body stub at end of file");
    }
    if trimmed.contains("extend type") && !trimmed.contains('{') {
        return Some("inserted empty extend body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "contract") {
        return Some("inserted empty contract body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "host") {
        return Some("inserted empty host body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "test") {
        return Some("inserted empty test body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "scope") {
        return Some("inserted empty scope body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "registry") {
        return Some("inserted empty registry body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "meta") {
        return Some("inserted empty meta block stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "skip") {
        return Some("inserted empty skip block stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "attribute") {
        return Some("inserted empty attribute body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "macro") {
        return Some("inserted empty macro body stub at end of file");
    }
    if stub_has_keyword_ident(trimmed, "mod") && !trimmed.contains('.') && !trimmed.contains('{') {
        return Some("inserted empty inline module body stub at end of file");
    }
    None
}

fn stub_has_keyword_ident(trimmed: &str, keyword: &str) -> bool {
    let mut i = 0usize;
    while i < trimmed.len() {
        if scan::keyword_at(trimmed, i, keyword) {
            let after = skip_ws(trimmed, i + keyword.len());
            return next_token_start(trimmed, after).is_some();
        }
        i += 1;
    }
    false
}
