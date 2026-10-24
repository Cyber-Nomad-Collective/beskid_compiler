//! Uses of deprecated items. Every entry carries [`DiagnosticTag::Deprecated`], so editors strike
//! the span through. No code is allocated yet.

use super::{CodeFamily, CodeInfo};

pub const FAMILY: CodeFamily = CodeFamily { name: "deprecated", first: "W2201", last: "W2299", codes: CODES };

pub const CODES: &[CodeInfo] = &[];
