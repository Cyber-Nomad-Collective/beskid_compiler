//! Synchronization-style recovery using grammar-inspired boundary tokens/keywords.

use crate::parser::Rule;

use super::{
    candidate::RepairCandidate, deletions, expected_tokens, ranking, scan, sync_primitives, syntax_primitives,
};

/// Generate boundary-aware statement boundary repairs (panic-mode style sync).
pub fn repairs(source: &str, error_pos: usize, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let error_pos = syntax_primitives::recovery_scan_pos(source, error_pos);
    let sync_keywords = sync_keywords_from_error(parse_error);
    let sync_tokens = syntax_primitives::recovery_follow_tokens(parse_error);

    candidates.extend(expected_token_repairs(source, error_pos, parse_error));
    candidates.extend(replacement_token_repairs(source, error_pos, parse_error));
    candidates.extend(single_token_deletion_repairs(source, error_pos, parse_error));

    if let Some(sync) = sync_boundary_candidate(source, error_pos, &sync_keywords, &sync_tokens)
        && !sync.suppress_semicolon
        && sync_primitives::should_insert_sync_semicolon(source, sync.pos, sync.boundary_char)
    {
        let insert_at = syntax_primitives::recovery_insert_position(source, sync.pos);
        candidates.push(RepairCandidate::insert(
            insert_at,
            ";",
            "inserted statement terminator at a grammar synchronization boundary",
            ranking::PRI_SYNC_BOUNDARY_SEMICOLON,
        ));
    }

    if let Some(punct_pos) = first_non_ws_after(source, error_pos)
        && is_punct_like_recoverable(source.as_bytes()[punct_pos])
    {
        candidates.push(RepairCandidate::delete(
            punct_pos,
            1,
            "removed unexpected punctuation while resynchronizing",
            ranking::PRI_SYNC_DELETE_TOKEN,
        ));
    }

    candidates
}

fn sync_boundary_candidate(
    source: &str,
    error_pos: usize,
    sync_keywords: &[&'static str],
    sync_tokens: &[&'static str],
) -> Option<sync_primitives::SyncBoundary> {
    if let Some(boundary) =
        sync_primitives::next_sync_boundary_with_follow_set(source, error_pos, sync_keywords, sync_tokens)
    {
        return Some(boundary);
    }
    sync_primitives::next_sync_boundary_with_follow_set(source, error_pos.saturating_add(1), sync_keywords, sync_tokens)
}

fn expected_token_repairs(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    let insert_at = scan::skip_ws(source, error_pos);
    let mut candidates = Vec::new();
    let scored = expected_tokens::expected_token_candidates(parse_error);
    for (insert_text, reason, confidence) in scored.into_iter().take(expected_tokens::MAX_EXPECTED_TOKEN_REPAIRS) {
        if matches_existing_token(source, insert_at, insert_text) {
            continue;
        }

        candidates.push(RepairCandidate::insert(
            insert_at,
            insert_text,
            reason,
            ranking::expected_token_priority(confidence),
        ));
    }
    candidates
}

fn replacement_token_repairs(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    let Some((replace_pos, replace_len)) = token_span_at_error(source, error_pos) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    let source_end = replace_pos.saturating_add(replace_len);
    if source_end > source.len() {
        return candidates;
    }
    let replaced = &source[replace_pos..source_end];
    let replaced_class = expected_tokens::replacement_token_class(replaced);

    let alternatives = expected_tokens::expected_token_candidates(parse_error);
    if alternatives.is_empty() {
        return candidates;
    }

    for (insert_text, _, confidence) in alternatives.into_iter().take(expected_tokens::MAX_EXPECTED_TOKEN_REPAIRS) {
        if !expected_tokens::is_simple_replacement_text(insert_text) {
            continue;
        }
        if !expected_tokens::is_replacement_credible(replaced, insert_text) {
            continue;
        }
        if source[replace_pos..source_end] == *insert_text {
            continue;
        }
        let replacement_class = expected_tokens::replacement_token_class(insert_text);
        if !expected_tokens::replacement_tokens_compatible(replaced_class, replacement_class) {
            continue;
        }
        let priority = ranking::replacement_priority(confidence, replaced, insert_text);
        candidates.push(RepairCandidate::replace(
            replace_pos,
            replace_len,
            insert_text,
            "replaced unexpected token with expected syntax token",
            priority,
        ));
    }

    candidates
}

fn single_token_deletion_repairs(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    deletions::single_token_deletion_repairs(source, error_pos, parse_error)
}

fn token_span_at_error(source: &str, error_pos: usize) -> Option<(usize, usize)> {
    let pos =
        scan::next_token_start(source, error_pos).or(if error_pos < source.len() { Some(error_pos) } else { None })?;
    if pos >= source.len() {
        return None;
    }
    let len = scan::token_len_at_raw(source, pos)?;
    Some((pos, (pos + len).min(source.len())))
}

fn matches_existing_token(source: &str, insert_at: usize, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if insert_at >= source.len() {
        return false;
    }
    let end = insert_at.saturating_add(token.len());
    end <= source.len() && source[insert_at..end] == *token
}

fn sync_keywords_from_error(parse_error: &pest::error::Error<Rule>) -> Vec<&'static str> {
    syntax_primitives::recovery_sync_keywords(parse_error)
}

fn first_non_ws_after(source: &str, from: usize) -> Option<usize> {
    let pos = scan::skip_ws(source, from);
    if pos < source.len() { Some(pos) } else { None }
}

fn is_punct_like_recoverable(byte: u8) -> bool {
    scan::is_delimiter_byte(byte) || scan::is_operator_byte(byte)
}
