//! Valid Beskid `Identifier` tokens for generated `.bd` field names.

/// Derive reserved tokens from the canonical Keyword grammar rather than a second list.
fn reserved_keywords() -> &'static Vec<&'static str> {
    static WORDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        let grammar = include_str!("../../beskid_analysis/src/beskid.pest");
        let body = grammar
            .split_once("\nKeyword = {")
            .expect("canonical Keyword rule")
            .1
            .split_once('}')
            .expect("Keyword end")
            .0;
        let mut words = Vec::new();
        for rule in
            body.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').filter(|token| token.ends_with("Keyword"))
        {
            let declaration = grammar
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("{rule} =")))
                .expect("canonical keyword declaration");
            let literal =
                declaration.split_once('"').expect("keyword literal").1.split_once('"').expect("keyword literal end").0;
            assert!(
                !literal.is_empty() && literal.chars().all(|c| c.is_ascii_alphabetic()),
                "unsupported canonical keyword literal"
            );
            words.push(literal);
        }
        assert!(!words.is_empty(), "canonical Keyword rule cannot be empty");
        words
    })
}

pub fn escape_beskid_ident(raw: &str) -> String {
    if reserved_keywords().iter().any(|keyword| raw == *keyword || raw.starts_with(&format!("{keyword}_"))) {
        format!("_{raw}")
    } else {
        raw.to_string()
    }
}

/// Enum names use the same case-sensitive canonical lexical authority as fields.
pub fn escape_beskid_variant_ident(raw: &str) -> String {
    escape_beskid_ident(raw)
}

/// Rust `snake_case` (or synthetic `field_0` / `variant_field_0`) to **lowerCamelCase** for Mod SDK
/// `.bd` field names, then [`escape_beskid_ident`] for keyword / `kw_` prefix safety.
pub fn rust_snake_to_beskid_field_camel(raw: &str) -> String {
    // Tuple placeholder names from the generator.
    if let Some(rest) = raw.strip_prefix("field_")
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return escape_beskid_ident(&format!("field{rest}"));
    }
    if let Some(rest) = raw.strip_prefix("variant_field_")
        && rest.chars().all(|c| c.is_ascii_digit())
    {
        return escape_beskid_ident(&format!("variantField{rest}"));
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
    use super::{escape_beskid_ident, escape_beskid_variant_ident, rust_snake_to_beskid_field_camel};

    #[test]
    fn escapes_keyword_variant_names_only() {
        assert_eq!(escape_beskid_variant_ident("This"), "_This");
        assert_eq!(escape_beskid_variant_ident("Complex"), "Complex");
        assert_eq!(escape_beskid_variant_ident("Thisish"), "Thisish");
    }

    #[test]
    fn camel_case_return_type() {
        assert_eq!(rust_snake_to_beskid_field_camel("return_type"), "returnType");
    }

    #[test]
    fn camel_case_contract_name() {
        assert_eq!(rust_snake_to_beskid_field_camel("contract_name"), "contractName");
    }

    #[test]
    fn camel_case_composition_keyword_prefixes() {
        assert_eq!(rust_snake_to_beskid_field_camel("scope_name"), "scopeName");
        assert_eq!(rust_snake_to_beskid_field_camel("host_name"), "hostName");
    }

    #[test]
    fn camel_field_index_placeholders() {
        assert_eq!(rust_snake_to_beskid_field_camel("field_0"), "field0");
        assert_eq!(rust_snake_to_beskid_field_camel("variant_field_1"), "variantField1");
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

#[cfg(test)]
mod canonical_keyword_v06_tests {
    #[test]
    fn v06_all_canonical_keyword_fields_are_escaped() {
        for keyword in super::reserved_keywords() {
            assert_eq!(super::escape_beskid_ident(keyword), format!("_{keyword}"));
        }
        assert_eq!(super::escape_beskid_ident("bulk"), "_bulk");
        assert_eq!(super::escape_beskid_ident("mutable"), "mutable");
        assert_eq!(super::escape_beskid_ident("whereBounds"), "whereBounds");
    }
}
