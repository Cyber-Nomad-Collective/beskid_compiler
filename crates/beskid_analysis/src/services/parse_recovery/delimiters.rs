//! Delimiter-oriented parse recovery candidates (`()[]{}<>`, fences).

use crate::parser::Rule;

use super::{RepairCandidate, skip_ws, unbalanced_delimiters};

/// Generate delimiter close/open repairs near the Pest error locus.
pub fn repairs(source: &str, error_pos: usize, _parse_error: &pest::error::Error<Rule>) -> Vec<RepairCandidate> {
    let mut candidates = Vec::new();
    let error_pos = error_pos.min(source.len());
    let insert_at = delimiter_insert_pos(source, error_pos);

    let (paren, bracket, brace, angle) = unbalanced_delimiters(source, error_pos);

    if paren > 0 {
        candidates.push(RepairCandidate::insert(insert_at, ")", "inserted missing closing parenthesis", 10));
    }
    if bracket > 0 {
        candidates.push(RepairCandidate::insert(insert_at, "]", "inserted missing closing bracket", 11));
    }
    if brace > 0 {
        candidates.push(RepairCandidate::insert(insert_at, "}", "inserted missing closing brace", 12));
    }
    if angle > 0 {
        candidates.push(RepairCandidate::insert(insert_at, ">", "inserted missing closing angle bracket", 13));
    }

    let boundary = skip_ws(source, error_pos);
    maybe_delete_extra_closer(
        &mut candidates,
        source,
        error_pos,
        b')',
        paren < 0,
        "removed extra closing parenthesis",
        14,
    );
    maybe_delete_extra_closer(
        &mut candidates,
        source,
        error_pos,
        b']',
        bracket < 0,
        "removed extra closing bracket",
        15,
    );
    maybe_delete_extra_closer(&mut candidates, source, error_pos, b'}', brace < 0, "removed extra closing brace", 16);
    maybe_delete_extra_closer(
        &mut candidates,
        source,
        error_pos,
        b'>',
        angle < 0,
        "removed extra closing angle bracket",
        17,
    );
    if boundary != error_pos {
        maybe_delete_extra_closer(
            &mut candidates,
            source,
            boundary,
            b')',
            paren < 0,
            "removed extra closing parenthesis",
            14,
        );
        maybe_delete_extra_closer(
            &mut candidates,
            source,
            boundary,
            b']',
            bracket < 0,
            "removed extra closing bracket",
            15,
        );
        maybe_delete_extra_closer(
            &mut candidates,
            source,
            boundary,
            b'}',
            brace < 0,
            "removed extra closing brace",
            16,
        );
        maybe_delete_extra_closer(
            &mut candidates,
            source,
            boundary,
            b'>',
            angle < 0,
            "removed extra closing angle bracket",
            17,
        );
    }

    if unclosed_string_before(source, error_pos) {
        candidates.push(RepairCandidate::insert(insert_at, "\"", "inserted missing closing string quote", 18));
    }

    if has_unclosed_code_fence(source) {
        candidates.push(RepairCandidate::insert(source.len(), "\n```\n", "inserted missing code fence closer", 19));
    }

    candidates
}

fn delimiter_insert_pos(source: &str, error_pos: usize) -> usize {
    let pos = skip_ws(source, error_pos);
    if pos >= source.len() { source.len() } else { pos }
}

fn maybe_delete_extra_closer(
    candidates: &mut Vec<RepairCandidate>,
    source: &str,
    pos: usize,
    closer: u8,
    extra: bool,
    reason: &'static str,
    priority: u8,
) {
    if !extra || pos >= source.len() || source.as_bytes()[pos] != closer {
        return;
    }
    candidates.push(RepairCandidate::delete(pos, 1, reason, priority));
}

fn unclosed_string_before(source: &str, through: usize) -> bool {
    let through = through.min(source.len());
    let bytes = source.as_bytes();
    let mut i = 0usize;
    let mut in_string = false;
    while i < through {
        if in_string {
            if bytes[i] == b'\\' {
                i = (i + 2).min(through);
                continue;
            }
            if bytes[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match bytes[i] {
            b'"' => {
                in_string = true;
                i += 1;
            }
            b'\'' => {
                i += 1;
                while i < through {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(through);
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < through && bytes[i + 1] == b'/' => {
                i += 2;
                while i < through && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < through && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < through && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(through);
            }
            _ => i += 1,
        }
    }
    in_string
}

fn has_unclosed_code_fence(source: &str) -> bool {
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut fence_count = 0u32;
    while i < len {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(len);
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(len);
                        continue;
                    }
                    if bytes[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(len);
            }
            b'`' if i + 2 < len && bytes[i + 1] == b'`' && bytes[i + 2] == b'`' => {
                fence_count += 1;
                i += 3;
            }
            _ => i += 1,
        }
    }
    fence_count % 2 == 1
}
