//! Macro expansion errors.
//!
//! Generated once from the emitting sources; add new codes here in code order.

use super::{CodeFamily, CodeInfo};

pub const FAMILY: CodeFamily = CodeFamily { name: "macros", first: "E1901", last: "E1999", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: "E1901", summary: "unknown macro", tags: &[] },
    CodeInfo { code: "E1902", summary: "macro argument count mismatch", tags: &[] },
    CodeInfo { code: "E1903", summary: "macro argument kind mismatch", tags: &[] },
    CodeInfo { code: "E1904", summary: "macro metavariable outside body", tags: &[] },
    CodeInfo { code: "E1905", summary: "macro expansion depth exceeded", tags: &[] },
    CodeInfo { code: "E1907", summary: "ambiguous macro name", tags: &[] },
    CodeInfo { code: "E1908", summary: "duplicate macro parameter", tags: &[] },
];
