//! Sync-boundary discovery and follow-token heuristics for panic-mode recovery.

use super::scan;
use super::syntax_primitives;

const NON_SEMI_PREVIOUS: &[u8] = b".=+-*/%:;{}[](),";

#[derive(Copy, Clone, Debug)]
pub(crate) struct SyncBoundary {
    pub(crate) pos: usize,
    pub(crate) boundary_char: Option<u8>,
    pub(crate) suppress_semicolon: bool,
}

/// Advance from `pos` to the next likely parser resynchronization boundary.
///
/// This keeps a strict boundary predicate while scanning over trivia:
/// strings/chars, comments, and whitespace are skipped, punctuation boundaries
/// are reported directly, and the remaining logic defers to shared syntax
/// primitives so boundary decisions stay consistent.
pub(crate) fn next_sync_boundary_with_follow_set(
    source: &str,
    mut pos: usize,
    sync_keywords: &[&'static str],
    follow_tokens: &[&'static str],
) -> Option<SyncBoundary> {
    let bytes = source.as_bytes();
    pos = scan::skip_ws(source, pos);
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
                pos = scan::skip_ws(source, pos);
                continue;
            }
            b'/' if pos + 1 < source.len() && bytes[pos + 1] == b'*' => {
                pos += 2;
                while pos + 1 < source.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                    pos += 1;
                }
                pos = (pos + 2).min(source.len());
                pos = scan::skip_ws(source, pos);
                continue;
            }
            b';' | b'}' | b')' | b']' | b'>' => {
                return Some(SyncBoundary { pos, boundary_char: Some(bytes[pos]), suppress_semicolon: false });
            }
            _ => {
                if let Some(_boundary_token_end) = follow_token_end(source, pos, follow_tokens) {
                    return Some(SyncBoundary { pos, boundary_char: None, suppress_semicolon: false });
                }

                if let Some((_, suppress_semicolon)) =
                    syntax_primitives::recoverable_sync_boundary_start(source, pos, sync_keywords)
                {
                    return Some(SyncBoundary { pos, boundary_char: None, suppress_semicolon });
                }
                if scan::is_ident_start(bytes[pos]) || bytes[pos] == b'_' {
                    pos = scan::skip_identifier(source, pos);
                } else {
                    pos += 1;
                }
            }
        }
        pos = scan::skip_ws(source, pos);
    }
    None
}

fn follow_token_end(source: &str, pos: usize, follow_tokens: &[&'static str]) -> Option<usize> {
    if follow_tokens.is_empty() {
        return None;
    }

    let token_start = scan::next_token_start(source, pos).or(if pos < source.len() { Some(pos) } else { None })?;
    if token_start >= source.len() {
        return None;
    }

    let token_end = scan::token_len_at_raw(source, token_start)?;
    let token_end = token_start + token_end;
    let token = &source[token_start..token_end];

    for follow_token in follow_tokens {
        if token == *follow_token {
            return Some(token_end);
        }
    }
    None
}

/// Decide whether inserting a semicolon at this boundary is safe.
///
/// This heuristic intentionally avoids insertion after punctuation that typically
/// already implies statement continuation and around block closers.
pub(crate) fn should_insert_sync_semicolon(source: &str, sync_pos: usize, boundary_byte: Option<u8>) -> bool {
    if boundary_byte == Some(b'}') {
        return false;
    }
    if sync_pos == 0 {
        return false;
    }

    if let Some(prev) = previous_non_ws_byte(source, sync_pos)
        && NON_SEMI_PREVIOUS.contains(&prev)
    {
        return false;
    }

    true
}

fn previous_non_ws_byte(source: &str, before: usize) -> Option<u8> {
    let mut prev = before;
    let bytes = source.as_bytes();
    while prev > 0 && bytes[prev - 1].is_ascii_whitespace() {
        prev -= 1;
    }
    if prev == 0 { None } else { Some(bytes[prev - 1]) }
}

#[cfg(test)]
mod tests {
    use super::next_sync_boundary_with_follow_set;

    #[test]
    fn follows_expected_follow_token_boundary() {
        let source = "let x = 1 return 0";
        let boundary = next_sync_boundary_with_follow_set(source, 0, &[], &["return"])
            .expect("expected parser sync boundary from follow token");
        assert_eq!(boundary.pos, 10);
        assert_eq!(boundary.boundary_char, None);
        assert!(!boundary.suppress_semicolon);
    }
}
