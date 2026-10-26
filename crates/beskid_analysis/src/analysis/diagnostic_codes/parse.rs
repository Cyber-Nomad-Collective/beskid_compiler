//! Parse errors, `E0101`–`E0199`. These constants are the only place parse codes are spelled; the
//! parser names them as `diagnostic_codes::parse::*`.

use super::{CodeFamily, CodeInfo};

pub const SYNTAX: &str = "E0101";
pub const MISSING_SEMICOLON: &str = "E0102";
pub const UNCLOSED_DELIMITER: &str = "E0103";
pub const HOST_WITHOUT_PARAMETERS: &str = "E0104";
pub const INCREMENT_OPERATOR: &str = "E0105";
pub const TYPED_LET: &str = "E0106";
pub const RESERVED_WORD: &str = "E0107";
pub const BLOCK_MATCH_ARM: &str = "E0108";
pub const UNKNOWN_ESCAPE: &str = "E0109";
pub const INVALID_UNICODE_ESCAPE: &str = "E0110";
// E0111 is retired: an unexpected syntax tree shape is internal, `internal::UNEXPECTED_TREE`.
pub const SELF_PARAMETER: &str = "E0112";
pub const EMPTY_SOURCE: &str = "E0113";
pub const LONG_CHARACTER_LITERAL: &str = "E0114";
pub const COMMA_BETWEEN_FIELDS: &str = "E0115";
pub const CONDITIONAL_WITHOUT_COLON: &str = "E0116";
pub const DETACHED_DOCUMENTATION: &str = "E0117";

pub const FAMILY: CodeFamily = CodeFamily { name: "parse", first: "E0101", last: "E0199", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: SYNTAX, summary: "unexpected token", tags: &[] },
    CodeInfo { code: MISSING_SEMICOLON, summary: "missing `;`", tags: &[] },
    CodeInfo { code: UNCLOSED_DELIMITER, summary: "unclosed delimiter", tags: &[] },
    CodeInfo { code: HOST_WITHOUT_PARAMETERS, summary: "host without a parameter list", tags: &[] },
    CodeInfo { code: INCREMENT_OPERATOR, summary: "`++` or `--` operator", tags: &[] },
    CodeInfo { code: TYPED_LET, summary: "type annotation on `let`", tags: &[] },
    CodeInfo { code: RESERVED_WORD, summary: "reserved word used as a name", tags: &[] },
    CodeInfo { code: BLOCK_MATCH_ARM, summary: "block used as a match arm value", tags: &[] },
    CodeInfo { code: UNKNOWN_ESCAPE, summary: "unknown string escape", tags: &[] },
    CodeInfo { code: INVALID_UNICODE_ESCAPE, summary: "invalid `\\u{...}` escape", tags: &[] },
    CodeInfo { code: SELF_PARAMETER, summary: "`self` parameter in a method", tags: &[] },
    CodeInfo { code: EMPTY_SOURCE, summary: "nothing to parse", tags: &[] },
    CodeInfo { code: LONG_CHARACTER_LITERAL, summary: "character literal with more than one character", tags: &[] },
    CodeInfo { code: COMMA_BETWEEN_FIELDS, summary: "comma between fields", tags: &[] },
    CodeInfo { code: CONDITIONAL_WITHOUT_COLON, summary: "conditional expression without `:`", tags: &[] },
    CodeInfo { code: DETACHED_DOCUMENTATION, summary: "documentation comment not attached to an item", tags: &[] },
];
