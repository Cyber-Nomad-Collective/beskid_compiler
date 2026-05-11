//! Valid Beskid `Identifier` tokens for generated `.bd` field names.

/// Beskid `Identifier` / `Keyword` overlap (see `beskid.pest`).
///
/// A trailing `_` is **not** sufficient (`attribute_` still starts a `Keyword` match in the
/// grammar). Reserved Rust names are prefixed with `_` so the Beskid lexer sees a normal
/// identifier (`_attribute`, `_type`, …).
pub const BESKID_RESERVED_IDENTIFIERS: &[&str] = &[
    "type", "enum", "contract", "attribute", "impl", "match", "event", "when", "if", "else",
    "while", "for", "in", "return", "break", "continue", "let", "mut", "mod", "use", "pub",
    "ref", "out", "test", "meta", "skip",
];

fn reserved_keyword_prefix_conflict(lower: &str) -> bool {
    for kw in BESKID_RESERVED_IDENTIFIERS {
        if lower == *kw {
            return true;
        }
        // `contract_name`, `type_name`, … still begin a `Keyword` match in `beskid.pest`.
        if lower.starts_with(&format!("{kw}_")) {
            return true;
        }
    }
    false
}

pub fn escape_beskid_ident(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if reserved_keyword_prefix_conflict(&lower) {
        format!("_{raw}")
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::escape_beskid_ident;

    #[test]
    fn escapes_exact_reserved_words() {
        assert_eq!(escape_beskid_ident("type"), "_type");
        assert_eq!(escape_beskid_ident("attribute"), "_attribute");
    }

    #[test]
    fn escapes_reserved_keyword_snake_prefix() {
        assert_eq!(escape_beskid_ident("contract_name"), "_contract_name");
        assert_eq!(escape_beskid_ident("type_name"), "_type_name");
    }

    #[test]
    fn leaves_unrelated_identifiers_unchanged() {
        assert_eq!(escape_beskid_ident("method_name"), "method_name");
        assert_eq!(escape_beskid_ident("payload"), "payload");
    }
}
