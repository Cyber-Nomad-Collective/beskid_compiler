//! Diagnostic code registry: metadata the renderers attach to a code.
//!
//! Codes are grouped in families, one file per family, so a slice that adds codes edits only its
//! own family file:
//!
//! - [`parse`]: `E0101`–`E0199`, parse errors.
//! - [`unused`]: unused and unreachable code; every entry carries [`DiagnosticTag::Unnecessary`].
//! - [`deprecated`]: uses of deprecated items; every entry carries [`DiagnosticTag::Deprecated`].
//!
//! A code needs an entry only when it carries metadata (tags today). Every code links to the one
//! registry page, [`REGISTRY_URL`]; per-code pages are planned for 0.7.

pub mod deprecated;
pub mod parse;
pub mod unused;

use super::diagnostics::DiagnosticTag;

/// The stable diagnostic-code registry page every code links to (no per-code anchors yet).
pub const REGISTRY_URL: &str =
    "https://beskid-lang.org/docs/standard/capabilities/compiler--semantic-pipeline--diagnostic-code-registry/";

/// Registry metadata of one code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeInfo {
    pub code: &'static str,
    /// A short noun phrase naming what the code reports ("unused item").
    pub summary: &'static str,
    /// Tags every diagnostic with this code carries.
    pub tags: &'static [DiagnosticTag],
}

/// One family: its entries and the inclusive code range they must stay in.
#[derive(Debug, Clone, Copy)]
pub struct CodeFamily {
    pub name: &'static str,
    pub first: &'static str,
    pub last: &'static str,
    pub codes: &'static [CodeInfo],
}

/// Every family of the registry.
pub const FAMILIES: &[CodeFamily] = &[parse::FAMILY, unused::FAMILY, deprecated::FAMILY];

/// Registry metadata for `code`, if the code has an entry.
pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    FAMILIES.iter().flat_map(|family| family.codes.iter()).find(|info| info.code == code)
}

/// Tags every diagnostic with `code` carries.
pub fn default_tags(code: &str) -> &'static [DiagnosticTag] {
    lookup(code).map_or(&[], |info| info.tags)
}

/// Whether `code` has the registry shape: `E` or `W` followed by four digits.
pub fn is_registry_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 5 && matches!(bytes[0], b'E' | b'W') && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// The documentation link for `code`: the registry page for every registry-shaped code.
pub fn docs_url(code: &str) -> Option<&'static str> {
    is_registry_code(code).then_some(REGISTRY_URL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(code: &str) -> u32 {
        code[1..].parse().expect("registry code digits")
    }

    #[test]
    fn every_family_entry_is_a_registry_code_inside_its_range() {
        for family in FAMILIES {
            assert!(is_registry_code(family.first) && is_registry_code(family.last), "{}", family.name);
            for info in family.codes {
                assert!(is_registry_code(info.code), "{} has malformed code {}", family.name, info.code);
                assert!(
                    info.code.as_bytes()[0] == family.first.as_bytes()[0]
                        && (number(family.first)..=number(family.last)).contains(&number(info.code)),
                    "{} lies outside family {} ({}..={})",
                    info.code,
                    family.name,
                    family.first,
                    family.last
                );
                assert!(!info.summary.is_empty(), "{} needs a summary", info.code);
            }
        }
    }

    #[test]
    fn codes_are_unique_across_families() {
        let mut seen = std::collections::HashSet::new();
        for info in FAMILIES.iter().flat_map(|family| family.codes.iter()) {
            assert!(seen.insert(info.code), "{} is registered twice", info.code);
        }
    }

    #[test]
    fn tag_families_tag_every_entry() {
        assert!(unused::FAMILY.codes.iter().all(|info| info.tags.contains(&DiagnosticTag::Unnecessary)));
        assert!(deprecated::FAMILY.codes.iter().all(|info| info.tags.contains(&DiagnosticTag::Deprecated)));
    }

    #[test]
    fn unused_codes_are_tagged_unnecessary() {
        for code in ["W1503", "W1403", "W2101"] {
            assert_eq!(default_tags(code), &[DiagnosticTag::Unnecessary], "{code}");
        }
        assert!(default_tags("E1214").is_empty());
        assert!(lookup("W1504").is_none(), "W2101 replaces W1504");
    }

    #[test]
    fn registry_codes_link_to_the_registry_page() {
        assert_eq!(docs_url("E0101"), Some(REGISTRY_URL));
        assert_eq!(docs_url("W2101"), Some(REGISTRY_URL));
        assert_eq!(docs_url("parse"), None);
        assert_eq!(docs_url("project-preparation"), None);
    }
}
