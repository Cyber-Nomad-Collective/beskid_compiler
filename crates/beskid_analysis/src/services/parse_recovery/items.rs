//! Item / signature stub recovery for incomplete top-level constructs.

use crate::parser::Rule;

use super::{candidate::RepairCandidate, lists, scan, syntax_primitives};
use super::scan::{next_token_start, skip_ws, unbalanced_delimiters};

const PRIORITY_USE_MOD_SEMI_EOF: u8 = 42;
const PRIORITY_USE_MOD_SEMI_BEFORE_NEXT: u8 = 43;
const PRIORITY_CONTRACT_METHOD_SEMI: u8 = 44;
const PRIORITY_ITEM_BRACE_EOF: u8 = 45;
const PRIORITY_ITEM_BRACE_ERROR: u8 = 46;
const PRIORITY_FN_BODY_STUB: u8 = 48;
const PRIORITY_EMPTY_BODY_STUB: u8 = 49;
const PRI_TYPE_FIELD_TRAILING_COMMA_DELETE: u8 = 50;
const PRI_TYPE_FIELD_TRAILING_COMMA_FIX: u8 = 51;
const PRI_ENUM_VARIANT_TRAILING_COMMA_DELETE: u8 = 52;
const PRI_ENUM_VARIANT_TRAILING_COMMA_FIX: u8 = 53;


/// Generate item-boundary repairs (closers / trailing `;`) near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let mut candidates = Vec::new();
    candidates.extend(unclosed_item_brace_repairs(source, error_pos));
    candidates.extend(type_and_enum_field_list_repairs(source, error_pos));
    candidates.extend(use_mod_semicolon_repairs(source, error_pos));
    candidates.extend(contract_method_semicolon_repairs(source, error_pos));
    candidates.extend(missing_function_body_repairs(source, error_pos, parse_error));
    candidates.extend(empty_stub_body_repairs(source, error_pos, parse_error));
    candidates
}

fn unclosed_item_brace_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
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

fn use_mod_semicolon_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
    let Some((kind, decl_start)) = find_use_or_mod_declaration(source, error_pos) else {
        return Vec::new();
    };
    if kind == ModDeclKind::Inline {
        return Vec::new();
    }

    let decl_end = declaration_end_without_semicolon(source, decl_start, error_pos);
    if has_semicolon_in_range(source, decl_start, decl_end) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let trimmed_end = source[..decl_end].trim_end().len();
    if trimmed_end > decl_start {
        out.push(RepairCandidate::insert(
            trimmed_end,
            ";",
            "inserted semicolon to complete use or mod declaration",
            PRIORITY_USE_MOD_SEMI_EOF,
        ));
    }

    let after_decl = skip_ws(source, decl_start + 3);
    if let Some(next_item) = next_item_keyword_start(source, after_decl)
        && next_item > decl_start
        && next_item <= source.len()
    {
        let insert_at = syntax_primitives::recovery_insert_position(source, next_item);
        out.push(RepairCandidate::insert(
            insert_at,
            ";",
            "inserted semicolon before next top-level item keyword",
            PRIORITY_USE_MOD_SEMI_BEFORE_NEXT,
        ));
    }
    out
}

fn type_and_enum_field_list_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
    let scan_pos = error_pos.min(source.len());
    let Some(open_pos) = syntax_primitives::find_unclosed_delimiter_before(source, scan_pos, b'{', b'}') else {
        return Vec::new();
    };
    let insert_at = scan::skip_ws(source, scan_pos);

    let mut out = Vec::new();
    match item_body_keyword_before_brace(source, open_pos) {
        Some("type") => {
            lists::trailing_separator_before_close_delimiter(
                source,
                scan_pos,
                insert_at,
                &mut out,
                b'{',
                b'}',
                |_source, open, _scan_pos| item_body_keyword_before_brace(source, open) == Some("type"),
                "field: word",
                PRI_TYPE_FIELD_TRAILING_COMMA_DELETE,
                PRI_TYPE_FIELD_TRAILING_COMMA_FIX,
                "removed trailing comma in type field list",
                "inserted placeholder type field after trailing comma",
            );
        }
        Some("enum") => {
            lists::trailing_separator_before_close_delimiter(
                source,
                scan_pos,
                insert_at,
                &mut out,
                b'{',
                b'}',
                |_source, open, _scan_pos| item_body_keyword_before_brace(source, open) == Some("enum"),
                "Value",
                PRI_ENUM_VARIANT_TRAILING_COMMA_DELETE,
                PRI_ENUM_VARIANT_TRAILING_COMMA_FIX,
                "removed trailing comma in enum variant list",
                "inserted placeholder enum variant after trailing comma",
            );
        }
        _ => {}
    }

    out
}

fn contract_method_semicolon_repairs(source: &str, error_pos: usize) -> Vec<RepairCandidate> {
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

fn missing_function_body_repairs(
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

fn empty_stub_body_repairs(
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

fn near_item_body_context(source: &str, error_pos: usize) -> bool {
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

#[derive(Copy, Clone, Eq, PartialEq)]
enum ModDeclKind {
    Path,
    Inline,
}

fn find_use_or_mod_declaration(source: &str, error_pos: usize) -> Option<(ModDeclKind, usize)> {
    let mut last: Option<(ModDeclKind, usize)> = None;
    let mut pos = 0usize;
    while pos < error_pos {
        if scan::keyword_at(source, pos, "use") {
            if !has_semicolon_in_range(source, pos, error_pos) {
                last = Some((ModDeclKind::Path, pos));
            }
        } else if scan::keyword_at(source, pos, "mod") {
            let kind = mod_declaration_kind(source, pos);
            if kind == ModDeclKind::Path && !has_semicolon_in_range(source, pos, error_pos) {
                last = Some((kind, pos));
            }
        }
        pos += 1;
    }
    last
}

fn mod_declaration_kind(source: &str, mod_pos: usize) -> ModDeclKind {
    let mut pos = skip_ws(source, mod_pos + 3);
    let Some(name_start) = next_token_start(source, pos) else {
        return ModDeclKind::Path;
    };
    pos = name_start;
    while pos < source.len() {
        let b = source.as_bytes()[pos];
        if b.is_ascii_alphanumeric() || b == b'_' {
            pos += 1;
            continue;
        }
        if b == b'.' {
            return ModDeclKind::Path;
        }
        break;
    }
    let after_name = skip_ws(source, pos);
    if after_name < source.len() && source.as_bytes()[after_name] == b'{' {
        ModDeclKind::Inline
    } else {
        ModDeclKind::Path
    }
}

fn declaration_end_without_semicolon(source: &str, decl_start: usize, error_pos: usize) -> usize {
    if let Some(next_item) = next_item_keyword_start(source, decl_start + 1)
        && next_item > decl_start
        && next_item <= error_pos.max(decl_start)
    {
        return next_item;
    }
    error_pos.max(decl_start).min(source.len())
}

fn has_semicolon_in_range(source: &str, start: usize, end: usize) -> bool {
    source.as_bytes()[start..end.min(source.len())].contains(&b';')
}

fn inside_contract_block(source: &str, error_pos: usize) -> bool {
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

fn find_close_paren_near(source: &str, error_pos: usize) -> Option<usize> {
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

fn looks_like_signature_close_paren(source: &str, close_paren: usize) -> bool {
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

fn item_body_keyword_before_brace(source: &str, brace_pos: usize) -> Option<&'static str> {
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

            if !best.map_or(true, |(best_pos, _)| pos > best_pos) {
                continue;
            }

            best = Some((pos, token));
        }
    }

    best.map(|(_, token)| token)
}

fn error_near_eof(source: &str, error_pos: usize) -> bool {
    let eof = source.trim_end().len();
    skip_ws(source, error_pos) >= eof.saturating_sub(1)
}

fn next_item_keyword_start(source: &str, from: usize) -> Option<usize> {
    let mut pos = from;
    while pos < source.len() {
        let token = next_token_start(source, pos)?;
        if scan::keyword_at(source, token, "pub") {
            pos = token + 3;
            continue;
        }
        for kw in syntax_primitives::ITEM_START_KEYWORDS {
            if scan::keyword_at(source, token, kw) {
                return Some(token);
            }
        }
        pos = token + 1;
    }
    None
}
