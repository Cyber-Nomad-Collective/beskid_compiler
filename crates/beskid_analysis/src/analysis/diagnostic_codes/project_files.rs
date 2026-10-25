//! Project files, lock files and materialization.
//!
//! Generated once from the emitting sources; add new codes here in code order.

use super::{CodeFamily, CodeInfo};

/// Project preparation failed without a structured diagnostic (for example a Mod artifact that
/// cannot be loaded).
pub const PREPARATION_FAILED: &str = "E3040";

pub const FAMILY: CodeFamily = CodeFamily { name: "project_files", first: "E3001", last: "E3099", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: "E3001", summary: "project file not found", tags: &[] },
    CodeInfo { code: "E3002", summary: "manifest cannot be read", tags: &[] },
    CodeInfo { code: "E3003", summary: "manifest parse error", tags: &[] },
    CodeInfo { code: "E3004", summary: "manifest validation error", tags: &[] },
    CodeInfo { code: "E3005", summary: "target not found", tags: &[] },
    CodeInfo { code: "E3006", summary: "dependency manifest not found", tags: &[] },
    CodeInfo { code: "E3007", summary: "dependency cycle", tags: &[] },
    CodeInfo { code: "E3008", summary: "unresolved external dependencies", tags: &[] },
    CodeInfo { code: "E3011", summary: "unsupported v1 dependency source", tags: &[] },
    CodeInfo { code: "E3020", summary: "lock file cannot be read", tags: &[] },
    CodeInfo { code: "E3021", summary: "lock file cannot be written", tags: &[] },
    CodeInfo { code: "E3022", summary: "lock file out of date", tags: &[] },
    CodeInfo { code: "E3023", summary: "lock file change in frozen mode", tags: &[] },
    CodeInfo { code: "E3024", summary: "retired lock file format", tags: &[] },
    CodeInfo { code: "E3030", summary: "materialized directory cannot be created", tags: &[] },
    CodeInfo { code: "E3031", summary: "materialized source cannot be copied", tags: &[] },
    CodeInfo { code: "E3032", summary: "shared Corelib does not match its seal", tags: &[] },
    CodeInfo { code: PREPARATION_FAILED, summary: "project preparation failed", tags: &[] },
];
