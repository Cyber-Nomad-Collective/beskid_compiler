//! Style, documentation and naming warnings. Unused and unreachable code lives in [`super::unused`].
//!
//! Generated once from the emitting sources; add new codes here in code order.

use super::{CodeFamily, CodeInfo};

pub const FAMILY: CodeFamily = CodeFamily { name: "lints", first: "W1000", last: "W1999", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: "W1103", summary: "shadowed local", tags: &[] },
    CodeInfo { code: "W1110", summary: "ambiguous import candidate", tags: &[] },
    CodeInfo { code: "W1203", summary: "implicit numeric cast", tags: &[] },
    CodeInfo { code: "W1240", summary: "discarded array growth or shrink", tags: &[] },
    CodeInfo { code: "W1610", summary: "unknown @arg parameter", tags: &[] },
    CodeInfo { code: "W1611", summary: "duplicate @arg", tags: &[] },
    CodeInfo { code: "W1612", summary: "invalid @arg/@returns placement", tags: &[] },
    CodeInfo { code: "W1613", summary: "redundant @returns", tags: &[] },
    CodeInfo { code: "W1614", summary: "unknown documentation directive", tags: &[] },
    CodeInfo { code: "W1615", summary: "unresolved documentation @ref", tags: &[] },
    CodeInfo { code: "W1620", summary: "invalid @variant placement", tags: &[] },
    CodeInfo { code: "W1621", summary: "unknown @variant name", tags: &[] },
    CodeInfo { code: "W1622", summary: "duplicate @variant", tags: &[] },
    CodeInfo { code: "W1623", summary: "invalid @par placement", tags: &[] },
    CodeInfo { code: "W1624", summary: "unknown @par type parameter", tags: &[] },
    CodeInfo { code: "W1625", summary: "duplicate @par", tags: &[] },
    CodeInfo { code: "W1630", summary: "type name not PascalCase", tags: &[] },
    CodeInfo { code: "W1631", summary: "enum variant not PascalCase", tags: &[] },
    CodeInfo { code: "W1632", summary: "field not lowerCamelCase", tags: &[] },
    CodeInfo { code: "W1633", summary: "callable not PascalCase", tags: &[] },
    CodeInfo { code: "W1634", summary: "module segment not PascalCase", tags: &[] },
    CodeInfo { code: "W1635", summary: "generic parameter not PascalCase", tags: &[] },
    CodeInfo { code: "W1636", summary: "binding not lowerCamelCase", tags: &[] },
    CodeInfo { code: "W1637", summary: "test name not PascalCase", tags: &[] },
    CodeInfo { code: "W1638", summary: "macro name not lowerCamelCase", tags: &[] },
    CodeInfo { code: "W1639", summary: "redundant enum constructor parens", tags: &[] },
];
