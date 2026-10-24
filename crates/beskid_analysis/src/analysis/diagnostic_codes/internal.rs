//! Internal compiler errors, `E2101`–`E2199`: conditions the compiler should never reach. Each one
//! is still reported as a coded diagnostic, never a panic. Not to be confused with `W2101` (unused
//! item) in [`super::unused`].

use super::{CodeFamily, CodeInfo};

pub const SEMANTIC_FACT_UNAVAILABLE: &str = "E2101";
pub const LOWERING_RULE_MISSING: &str = "E2102";
pub const LOWERING_INVARIANT: &str = "E2103";
pub const MOD_SEMANTIC_FAILURE: &str = "E2104";
pub const BUILD_FAILURE: &str = "E2105";
/// The parser accepted the text but the syntax tree has a shape the builder does not expect
/// (formerly the parse code `E0111`).
pub const UNEXPECTED_TREE: &str = "E2106";
/// The formatter failed on a file that parses (output does not parse or is not stable).
pub const FORMATTER_FAILURE: &str = "E2107";

pub const FAMILY: CodeFamily = CodeFamily { name: "internal", first: "E2101", last: "E2199", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: SEMANTIC_FACT_UNAVAILABLE, summary: "internal error: semantic fact unavailable", tags: &[] },
    CodeInfo { code: LOWERING_RULE_MISSING, summary: "internal error: lowering rule missing", tags: &[] },
    CodeInfo { code: LOWERING_INVARIANT, summary: "internal error: lowering invariant broken", tags: &[] },
    CodeInfo { code: MOD_SEMANTIC_FAILURE, summary: "internal error: Mod semantic failure", tags: &[] },
    CodeInfo { code: BUILD_FAILURE, summary: "internal error: build failure", tags: &[] },
    CodeInfo { code: UNEXPECTED_TREE, summary: "internal error: unexpected syntax tree shape", tags: &[] },
    CodeInfo { code: FORMATTER_FAILURE, summary: "internal error: formatter failure", tags: &[] },
];
