//! Parse errors, `E0101`–`E0199`.
//!
//! Owned by the parse-wording work (0.6.7 slice S2): add one [`CodeInfo`] per parse error below,
//! in code order. Parse codes carry no tags.

use super::{CodeFamily, CodeInfo};

pub const FAMILY: CodeFamily = CodeFamily { name: "parse", first: "E0101", last: "E0199", codes: CODES };

pub const CODES: &[CodeInfo] = &[
    CodeInfo { code: "E0101", summary: "unexpected token", tags: &[] },
    CodeInfo { code: "E0102", summary: "missing `;`", tags: &[] },
    CodeInfo { code: "E0103", summary: "unclosed delimiter", tags: &[] },
    CodeInfo { code: "E0104", summary: "host without a parameter list", tags: &[] },
    CodeInfo { code: "E0105", summary: "`++` or `--` operator", tags: &[] },
    CodeInfo { code: "E0106", summary: "type annotation on `let`", tags: &[] },
    CodeInfo { code: "E0107", summary: "reserved word used as a name", tags: &[] },
    CodeInfo { code: "E0108", summary: "block used as a match arm value", tags: &[] },
    CodeInfo { code: "E0109", summary: "unknown string escape", tags: &[] },
    CodeInfo { code: "E0110", summary: "invalid `\\u{...}` escape", tags: &[] },
    CodeInfo { code: "E0111", summary: "unexpected syntax tree shape", tags: &[] },
    CodeInfo { code: "E0112", summary: "`self` parameter in a method", tags: &[] },
    CodeInfo { code: "E0113", summary: "nothing to parse", tags: &[] },
];

#[cfg(test)]
mod tests {
    use super::CODES;
    use crate::parsing::diagnose::codes;

    #[test]
    fn every_parse_code_the_parser_emits_is_registered() {
        for code in [
            codes::SYNTAX,
            codes::MISSING_SEMICOLON,
            codes::UNCLOSED_DELIMITER,
            codes::HOST_WITHOUT_PARAMETERS,
            codes::INCREMENT_OPERATOR,
            codes::TYPED_LET,
            codes::RESERVED_WORD,
            codes::BLOCK_MATCH_ARM,
            codes::UNKNOWN_ESCAPE,
            codes::INVALID_UNICODE_ESCAPE,
            codes::UNEXPECTED_TREE,
            codes::SELF_PARAMETER,
            codes::EMPTY_SOURCE,
        ] {
            assert!(CODES.iter().any(|info| info.code == code), "{code} is not registered");
        }
    }
}
