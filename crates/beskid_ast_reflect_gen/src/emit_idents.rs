//! Valid Beskid `Identifier` tokens for generated `.bd` field names.

/// Beskid `Identifier` / `Keyword` overlap (see `beskid.pest`).
///
/// A trailing `_` is **not** sufficient (`attribute_` still starts a `Keyword` match in the
/// grammar). Reserved Rust names are prefixed with `_` so the Beskid lexer sees a normal
/// identifier (`_attribute`, `_type`, …).
pub const BESKID_RESERVED_IDENTIFIERS: &[&str] = &[
    "type",
    "enum",
    "contract",
    "attribute",
    "impl",
    "match",
    "event",
    "when",
    "if",
    "else",
    "while",
    "for",
    "in",
    "return",
    "break",
    "continue",
    "let",
    "mut",
    "mod",
    "use",
    "pub",
    "ref",
    "out",
    "test",
    "meta",
    "skip",
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

/// Rust `snake_case` (or synthetic `field_0` / `variant_field_0`) to **lowerCamelCase** for Mod SDK
/// `.bd` field names, then [`escape_beskid_ident`] for keyword / `kw_` prefix safety.
pub fn rust_snake_to_beskid_field_camel(raw: &str) -> String {
    // Tuple placeholder names from the generator.
    if let Some(rest) = raw.strip_prefix("field_") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return escape_beskid_ident(&format!("field{rest}"));
        }
    }
    if let Some(rest) = raw.strip_prefix("variant_field_") {
        if rest.chars().all(|c| c.is_ascii_digit()) {
            return escape_beskid_ident(&format!("variantField{rest}"));
        }
    }
    if raw == "payload" {
        return escape_beskid_ident("payload");
    }

    let parts: Vec<&str> = raw.split('_').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return escape_beskid_ident("_");
    }
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i == 0 {
            out.push_str(&p.to_lowercase());
        } else {
            let mut ch = p.chars();
            if let Some(c) = ch.next() {
                out.extend(c.to_uppercase());
                out.push_str(&ch.as_str().to_lowercase());
            }
        }
    }
    escape_beskid_ident(&out)
}

#[cfg(test)]
mod tests {
    use super::{escape_beskid_ident, rust_snake_to_beskid_field_camel};

    #[test]
    fn camel_case_return_type() {
        assert_eq!(
            rust_snake_to_beskid_field_camel("return_type"),
            "returnType"
        );
    }

    #[test]
    fn camel_case_contract_name() {
        assert_eq!(
            rust_snake_to_beskid_field_camel("contract_name"),
            "contractName"
        );
    }

    #[test]
    fn camel_field_index_placeholders() {
        assert_eq!(rust_snake_to_beskid_field_camel("field_0"), "field0");
        assert_eq!(
            rust_snake_to_beskid_field_camel("variant_field_1"),
            "variantField1"
        );
    }

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
