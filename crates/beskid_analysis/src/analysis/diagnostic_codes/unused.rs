//! Unused and unreachable code. Every entry carries [`DiagnosticTag::Unnecessary`], so editors
//! fade the span out.

use super::{CodeFamily, CodeInfo};
use crate::analysis::diagnostics::DiagnosticTag;

const UNNECESSARY: &[DiagnosticTag] = &[DiagnosticTag::Unnecessary];

pub const FAMILY: CodeFamily = CodeFamily { name: "unused", first: "W1000", last: "W2199", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: "W1403", summary: "unreachable statement", tags: UNNECESSARY },
    CodeInfo { code: "W1503", summary: "unused import", tags: UNNECESSARY },
    // Unused item, including unused private items (replaces W1504; semantic slice S3 emits it).
    CodeInfo { code: "W2101", summary: "unused item", tags: UNNECESSARY },
];
