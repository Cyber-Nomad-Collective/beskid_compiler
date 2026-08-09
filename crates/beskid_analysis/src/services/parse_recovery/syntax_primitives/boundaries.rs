use super::super::scan::{self, skip_ws};
use super::keyword_rules::{CONTROL_FLOW_KEYWORDS, KEYWORDS, NON_SEMI_SYNC_KEYWORDS, SYNC_KEYWORDS};

pub(crate) fn is_line_start(source: &str, pos: usize) -> bool {
    let mut cursor = pos.min(source.len());
    while cursor > 0 && source.as_bytes()[cursor - 1].is_ascii_whitespace() {
        if source.as_bytes()[cursor - 1] == b'\n' {
            return true;
        }
        cursor -= 1;
    }
    cursor == 0 || source.as_bytes()[cursor - 1] == b'\n'
}

pub(crate) fn is_for_clause_in_keyword(source: &str, pos: usize) -> bool {
    let mut line_start = pos;
    while line_start > 0
        && source.as_bytes()[line_start - 1] != b'\n'
        && source.as_bytes()[line_start - 1].is_ascii_whitespace()
    {
        line_start -= 1;
    }
    if line_start > 0
        && !source.as_bytes()[line_start - 1].is_ascii_whitespace()
        && source.as_bytes()[line_start - 1] != b'\n'
    {
        return false;
    }

    let prev = line_start.saturating_sub(1);
    if prev == 0 {
        return false;
    }

    let mut token_start = prev;
    while token_start > 0 && scan::is_ident_continue(source.as_bytes()[token_start - 1]) {
        token_start -= 1;
    }
    &source[token_start..prev] == "for"
}

pub(crate) fn is_recoverable_sync_keyword(source: &str, pos: usize, keyword: &str) -> bool {
    if keyword == "as" {
        return is_line_start(source, pos);
    }
    if keyword == "in" {
        return !is_for_clause_in_keyword(source, pos);
    }
    true
}

pub(crate) fn is_recoverable_statement_start(source: &str, pos: usize, keyword: &str) -> bool {
    match keyword {
        "as" | "mut" | "pub" => is_line_start(source, pos),
        "in" => !is_for_clause_in_keyword(source, pos),
        _ => true,
    }
}

pub(crate) fn should_skip_sync_semicolon(keyword: &str) -> bool {
    NON_SEMI_SYNC_KEYWORDS.iter().any(|item| item == &keyword)
}

pub(crate) fn is_top_level_at(source: &str, pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let (paren, bracket, brace, angle) = scan::unbalanced_delimiters(source, pos);
    paren == 0 && bracket == 0 && brace == 0 && angle == 0
}

pub(crate) fn recovery_scan_pos(source: &str, error_pos: usize) -> usize {
    if error_pos == 0 {
        return source.trim_end().len();
    }
    error_pos.min(source.len())
}

pub(crate) fn recovery_insert_position(source: &str, boundary_pos: usize) -> usize {
    let mut pos = boundary_pos.min(source.len());
    while pos > 0 && source.as_bytes()[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    pos
}

pub(crate) fn is_recoverable_identifier_statement_starter(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    if !is_top_level_at(source, pos) {
        return false;
    }
    if !is_line_start(source, pos) {
        return false;
    }
    let bytes = source.as_bytes();
    if !scan::is_ident_start(bytes[pos]) && bytes[pos] != b'_' {
        return false;
    }
    for keyword in SYNC_KEYWORDS {
        if scan::keyword_at(source, pos, keyword) {
            return false;
        }
    }
    true
}

pub(crate) fn is_recoverable_expression_statement_starter(source: &str, pos: usize) -> bool {
    if pos >= source.len() {
        return false;
    }
    if !is_line_start(source, pos) {
        return false;
    }
    if !scan::looks_like_expression_start(source, pos) {
        return false;
    }
    let bytes = source.as_bytes();
    if bytes[pos] == b'_' || scan::is_ident_start(bytes[pos]) {
        let end = scan::skip_identifier(source, pos);
        if end > pos && is_keyword_text(&source[pos..end]) {
            return false;
        }
    }
    true
}

pub(crate) fn recovery_source_has_fallback_control_flow_hint(
    source: &str,
    error_pos: usize,
    keywords: &[&str],
) -> bool {
    let error_pos = error_pos.min(source.len());
    let trimmed = source[..error_pos].trim_end();
    if trimmed.trim().is_empty() {
        return false;
    }
    let mut latest = None::<(usize, &str)>;
    let bytes = trimmed.as_bytes();
    for &keyword in keywords {
        let Some(kw_pos) = scan::find_keyword_backward(trimmed, trimmed.len(), keyword) else {
            continue;
        };
        if kw_pos > 0 && scan::is_ident_continue(bytes[kw_pos - 1]) {
            continue;
        }
        let kw_end = kw_pos + keyword.len();
        if kw_end < bytes.len() && scan::is_ident_continue(bytes[kw_end]) {
            continue;
        }
        match latest {
            Some((current, _)) if current >= kw_pos => {}
            _ => latest = Some((kw_pos, keyword)),
        }
    }
    let Some((kw_pos, keyword)) = latest else {
        return false;
    };

    let after_kw = skip_ws(trimmed, kw_pos + keyword.len());
    if after_kw >= trimmed.len() {
        return false;
    }
    if trimmed.as_bytes()[kw_pos + keyword.len()] == b'=' {
        return false;
    }
    let tail = trimmed[after_kw..trimmed.len()].trim();
    if tail.is_empty() || tail.starts_with("=>") || tail.ends_with(':') || tail.ends_with('=') || tail.ends_with("=>") {
        return false;
    }
    if tail.contains('{') {
        return false;
    }
    true
}

/// Shared sync-boundary predicate used by sync and statement-start discovery.
///
/// Returns `Some` when `pos` looks like a recoverable statement/expression boundary.
///
/// The first tuple item is:
/// - `Some(keyword)` if the boundary starts with a sync keyword from `sync_keywords`.
/// - `None` for non-keyword statement/expression starters.
pub(crate) fn recoverable_sync_boundary_start<'a>(
    source: &str,
    pos: usize,
    sync_keywords: &'a [&'a str],
) -> Option<(Option<&'a str>, bool)> {
    for &keyword in sync_keywords {
        if scan::keyword_at(source, pos, keyword) && is_recoverable_sync_keyword(source, pos, keyword) {
            return Some((Some(keyword), should_skip_sync_semicolon(keyword)));
        }
    }

    if is_recoverable_identifier_statement_starter(source, pos)
        || is_recoverable_expression_statement_starter(source, pos)
    {
        return Some((None, false));
    }

    None
}

/// Scan for recoverable top-level statement starts in syntax order.
pub(crate) fn top_level_statement_starts(source: &str, from: usize, keywords: &[&str]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut pos = from.min(source.len());
    while pos < source.len() {
        pos = skip_ws(source, pos);
        if pos >= source.len() {
            break;
        }

        let Some((keyword, _)) = recoverable_sync_boundary_start(source, pos, keywords) else {
            pos += 1;
            continue;
        };

        if let Some(keyword) = keyword {
            if is_recoverable_statement_start(source, pos, keyword) {
                starts.push(pos);
                pos = pos.saturating_add(keyword.len());
                continue;
            }
        } else {
            starts.push(pos);
            let next_pos = scan::next_token_start(source, pos + 1).unwrap_or(source.len());
            pos = next_pos.max(pos + 1);
            continue;
        }

        pos += 1;
    }
    starts
}

pub(crate) fn control_flow_keyword_len(source: &str, kw_pos: usize) -> Option<usize> {
    for &keyword in CONTROL_FLOW_KEYWORDS {
        if source[kw_pos..].starts_with(keyword) {
            return Some(keyword.len());
        }
    }
    None
}

pub(crate) fn is_keyword_text(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    KEYWORDS.contains(&text)
}
