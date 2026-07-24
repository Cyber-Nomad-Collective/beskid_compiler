//! Shared parse-recovery primitives and domain candidate generators.
//!
//! Domains emit [`RepairCandidate`]s; the orchestrator in [`super::parse`] applies
//! them, dedupes repaired sources, and retries a strict parse (capped).

use std::collections::HashSet;

use pest::error::InputLocation;

use crate::analysis::diagnostics::SemanticDiagnostic;
use crate::parser::Rule;
use crate::syntax::SpanInfo;

use super::diagnostics_emit::parse_recovery_diagnostic;

mod delimiters;
mod expressions;
mod items;
mod separators;
pub(crate) mod utils;

/// Maximum unique repaired sources tried per recovery attempt.
pub const MAX_RECOVERY_CANDIDATES: usize = 16;

/// Atomic text edit applied at a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairKind {
    Insert { text: &'static str },
    Delete { len: usize },
}

/// One recovery attempt: a single repair plus human-readable reason and try order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairCandidate {
    pub position: usize,
    pub kind: RepairKind,
    pub reason: &'static str,
    /// Lower values are tried first.
    pub priority: u8,
}

impl RepairCandidate {
    pub fn insert(position: usize, text: &'static str, reason: &'static str, priority: u8) -> Self {
        Self {
            position,
            kind: RepairKind::Insert { text },
            reason,
            priority,
        }
    }

    pub fn delete(position: usize, len: usize, reason: &'static str, priority: u8) -> Self {
        Self {
            position,
            kind: RepairKind::Delete { len },
            reason,
            priority,
        }
    }
}

/// Apply a single repair to `source`. Returns `None` if the edit is out of range.
pub fn apply_repair(source: &str, candidate: &RepairCandidate) -> Option<String> {
    let pos = candidate.position;
    if pos > source.len() {
        return None;
    }
    match candidate.kind {
        RepairKind::Insert { text } => {
            let mut repaired = String::with_capacity(source.len() + text.len());
            repaired.push_str(&source[..pos]);
            repaired.push_str(text);
            let safe_pos = if source.is_char_boundary(pos) { pos } else { source.len() };
            repaired.push_str(&source[safe_pos..]);
            Some(repaired)
        }
        RepairKind::Delete { len } => {
            let end = pos.saturating_add(len);
            if end > source.len() {
                return None;
            }
            let mut repaired = String::with_capacity(source.len().saturating_sub(len));
            repaired.push_str(&source[..pos]);
            let safe_end = if source.is_char_boundary(end) { end } else { source.len() };
            repaired.push_str(&source[safe_end..]);
            Some(repaired)
        }
    }
}

pub fn error_byte_pos(parse_error: &pest::error::Error<Rule>) -> usize {
    match parse_error.location {
        InputLocation::Pos(pos) => pos,
        InputLocation::Span((start, _)) => start,
    }
}

pub fn skip_ws(source: &str, from: usize) -> usize {
    let bytes = source.as_bytes();
    let mut pos = from.min(source.len());
    while pos < source.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

/// First non-whitespace token start at or after `from`, preferring statement/item boundaries.
pub fn next_token_start(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut pos = skip_ws(source, from);
    if pos >= source.len() {
        return None;
    }
    if bytes[pos] == b'}'
        || bytes[pos].is_ascii_alphanumeric()
        || bytes[pos] == b'_'
        || bytes[pos] == b'{'
        || bytes[pos] == b'@'
        || bytes[pos] == b'('
        || bytes[pos] == b'['
        || bytes[pos] == b'<'
    {
        return Some(pos);
    }
    while pos < source.len() {
        let b = bytes[pos];
        if b == b'}'
            || b.is_ascii_alphanumeric()
            || b == b'_'
            || b == b'{'
            || b == b'@'
            || b == b'('
            || b == b'['
            || b == b'<'
        {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

/// Net open counts for `()[]{}<>` from the start of `source` through `through` (exclusive).
/// Angle brackets are best-effort (generics vs comparison).
pub fn unbalanced_delimiters(source: &str, through: usize) -> (i32, i32, i32, i32) {
    let through = through.min(source.len());
    let bytes = source.as_bytes();
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut angle = 0i32;
    let mut i = 0usize;
    while i < through {
        // Skip string / char / line comments roughly so fence recovery stays stable.
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < through {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(through);
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
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
                continue;
            }
            b'/' if i + 1 < through && bytes[i + 1] == b'/' => {
                i += 2;
                while i < through && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < through && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < through && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(through);
                continue;
            }
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'[' => bracket += 1,
            b']' => bracket -= 1,
            b'{' => brace += 1,
            b'}' => brace -= 1,
            b'<' => angle += 1,
            b'>' => angle -= 1,
            _ => {}
        }
        i += 1;
    }
    (paren, bracket, brace, angle)
}

/// Collect, priority-sort, dedupe, and cap recovery source candidates with diagnostics.
pub fn collect_repair_candidates(
    source_name: &str,
    source: &str,
    parse_error: &pest::error::Error<Rule>,
) -> Vec<(String, Vec<SemanticDiagnostic>)> {
    let error_pos = error_byte_pos(parse_error).min(source.len());
    let mut repairs = Vec::new();
    repairs.extend(delimiters::repairs(source, error_pos, parse_error));
    repairs.extend(separators::repairs(source, error_pos, parse_error));
    repairs.extend(items::repairs(source, error_pos, parse_error));
    repairs.extend(expressions::repairs(source, error_pos, parse_error));

    repairs.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.position.cmp(&b.position))
            .then_with(|| a.reason.cmp(b.reason))
    });

    let mut out: Vec<(String, Vec<SemanticDiagnostic>)> = Vec::new();
    let mut seen = HashSet::new();

    for repair in repairs {
        let Some(repaired) = apply_repair(source, &repair) else {
            continue;
        };
        if !seen.insert(repaired.clone()) {
            continue;
        }
        let diagnostics = vec![parse_recovery_diagnostic(
            source_name,
            source,
            SpanInfo::from_byte_range_in_source(source, repair.position, repair.position),
            repair.reason,
        )];
        out.push((repaired, diagnostics));
        if out.len() >= MAX_RECOVERY_CANDIDATES {
            break;
        }
    }

    if out.is_empty() {
        out.push((source.to_string(), Vec::new()));
    }
    out
}
