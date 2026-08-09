use super::super::scan::{skip_ws, unbalanced_delimiters};
use super::super::{candidate::RepairCandidate, scan};
use super::priorities::{PRI_LAMBDA_BODY, PRI_MATCH_ARM_COMMA, PRI_MATCH_ARROW, PRI_MATCH_CLOSE};
use super::scanner_context::find_match_block_brace;

pub(super) fn match_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
    let Some(match_brace) = find_match_block_brace(source, error_pos) else {
        return;
    };

    let (_, _, brace, _) = unbalanced_delimiters(source, error_pos);
    if brace > 0 {
        candidates.push(RepairCandidate::insert(
            insert_at,
            "}",
            "closed incomplete match expression block",
            PRI_MATCH_CLOSE,
        ));
    }

    if missing_match_arm_arrow(source, match_brace, error_pos) {
        let arrow_pos = match_arm_arrow_pos(source, match_brace, error_pos);
        candidates.push(RepairCandidate::insert(
            arrow_pos,
            "=>",
            "inserted missing match arm fat arrow",
            PRI_MATCH_ARROW,
        ));
    }

    if trailing_incomplete_match_arm(source, match_brace, error_pos) {
        candidates.push(RepairCandidate::insert(
            insert_at,
            ",",
            "inserted comma after incomplete match arm",
            PRI_MATCH_ARM_COMMA,
        ));
    }
}

pub(super) fn lambda_repairs(source: &str, error_pos: usize, insert_at: usize, candidates: &mut Vec<RepairCandidate>) {
    if !lambda_missing_body(source, error_pos) {
        return;
    }

    candidates.push(RepairCandidate::insert(
        insert_at,
        "{}",
        "inserted empty block as placeholder lambda body",
        PRI_LAMBDA_BODY,
    ));
}

fn missing_match_arm_arrow(source: &str, match_brace: usize, error_pos: usize) -> bool {
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    let segment = &source[arm_start..error_pos];
    if segment.contains("=>") {
        return false;
    }
    arm_segment_looks_like_pattern(segment)
}

fn match_arm_arrow_pos(source: &str, match_brace: usize, error_pos: usize) -> usize {
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    skip_ws(source, error_pos).max(arm_start)
}

pub(super) fn current_match_arm_start(source: &str, match_brace: usize, error_pos: usize) -> usize {
    let slice = &source[match_brace + 1..error_pos.min(source.len())];
    if let Some(comma) = slice.rfind(',') { match_brace + 1 + comma + 1 } else { match_brace + 1 }
}

fn arm_segment_looks_like_pattern(segment: &str) -> bool {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('_') {
        return true;
    }
    trimmed.chars().any(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '(')
}

fn trailing_incomplete_match_arm(source: &str, match_brace: usize, error_pos: usize) -> bool {
    let tail_start = skip_ws(source, error_pos);
    if tail_start < source.len() {
        return false;
    }
    let (_, _, brace, _) = unbalanced_delimiters(source, error_pos);
    if brace <= 0 {
        return false;
    }
    let arm_start = current_match_arm_start(source, match_brace, error_pos);
    let segment = source[arm_start..error_pos].trim();
    if segment.is_empty() {
        return false;
    }
    segment.contains("=>") && !segment.ends_with(',')
}

fn lambda_missing_body(source: &str, error_pos: usize) -> bool {
    let Some(arrow) = find_lambda_arrow(source, error_pos) else {
        return false;
    };
    if find_match_block_brace(source, arrow).is_some() {
        return false;
    }
    let after_arrow = skip_ws(source, arrow + 2);
    let tail = skip_ws(source, error_pos);
    tail >= source.trim_end().len() && after_arrow >= tail
}

fn find_lambda_arrow(source: &str, through: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = through.min(source.len());
    while i > 1 {
        i -= 1;
        if bytes[i] == b'>' && i > 0 && bytes[i - 1] == b'=' {
            let arrow = i - 1;
            if lambda_arrow_is_parameter_tail(source, arrow) {
                return Some(arrow);
            }
        }
    }
    None
}

fn lambda_arrow_is_parameter_tail(source: &str, arrow: usize) -> bool {
    let mut pos = arrow;
    while pos > 0 {
        pos -= 1;
        let b = source.as_bytes()[pos];
        if b.is_ascii_whitespace() {
            continue;
        }
        return b == b')' || scan::is_ident_continue(b);
    }
    false
}
