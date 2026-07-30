//! Single-token deletion recovery strategies.

use crate::parser::Rule;

use super::{candidate::RepairCandidate, heuristics, scan};

pub(crate) fn single_token_deletion_repairs(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<RepairCandidate> {
    if let Some(recovery) = direct_single_token_recovery(source, error_pos, parse_error) {
        return vec![recovery];
    }
    if matches!(parse_error.location, pest::error::InputLocation::Pos(0)) {
        return duplicate_token_repairs(source, parse_error);
    }
    Vec::new()
}

fn direct_single_token_recovery(
    source: &str,
    error_pos: usize,
    parse_error: &pest::error::Error<Rule>,
) -> Option<RepairCandidate> {
    let (unexpected_pos, unexpected_len) = token_span_at_error(source, error_pos)?;
    let next_start = scan::next_token_start(source, unexpected_pos.saturating_add(unexpected_len))?;
    if next_start >= source.len() || next_start <= unexpected_pos {
        return None;
    }
    let next_len = scan::token_len_at_raw(source, next_start)?;
    let next_end = (next_start + next_len).min(source.len());
    let unexpected = &source[unexpected_pos..(unexpected_pos + unexpected_len).min(source.len())];
    let next_token = &source[next_start..next_end];
    if !heuristics::is_single_delete_candidate_token(unexpected) {
        return None;
    }
    if next_token.is_empty() {
        return None;
    }
    if !heuristics::can_recover_single_token_deletion(source, next_start, unexpected, next_token, parse_error) {
        return None;
    }

    Some(RepairCandidate::delete(
        unexpected_pos,
        unexpected_len,
        "removed unexpected token via single-token deletion recovery",
        heuristics::PRI_SYNC_SINGLE_TOKEN_DELETE,
    ))
}

fn duplicate_token_repairs(source: &str, parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let mut pos = 0usize;
    while pos < source.len() {
        let Some(first_len) = scan::token_len_at_raw(source, pos) else {
            break;
        };
        if first_len == 0 {
            break;
        }

        let second_pos = pos + first_len;
        if second_pos >= source.len() {
            break;
        }
        let Some(second_len) = scan::token_len_at_raw(source, second_pos) else {
            break;
        };
        let first = &source[pos..pos + first_len];
        let second = &source[second_pos..second_pos + second_len];
        if first_len == second_len
            && first == second
            && first_len == 1
            && heuristics::is_single_delete_candidate_token(first)
            && scan::next_token_start(source, second_pos + second_len).is_some()
        {
            let after_second = scan::next_token_start(source, second_pos + second_len).unwrap_or(source.len());
            let Some(next_len) = scan::token_len_at_raw(source, after_second) else {
                pos += first_len;
                continue;
            };
            let next_end = (after_second + next_len).min(source.len());
            let next_token = &source[after_second..next_end];
            let can_recover = !next_token.is_empty()
                && heuristics::can_recover_single_token_deletion(source, after_second, first, next_token, parse_error);
            if can_recover {
                candidates.push(RepairCandidate::delete(
                    pos,
                    first_len,
                    "removed duplicate token via single-token recovery fallback",
                    heuristics::PRI_SYNC_SINGLE_TOKEN_DELETE,
                ));
            }
        }

        pos += first_len;
    }
    candidates
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
