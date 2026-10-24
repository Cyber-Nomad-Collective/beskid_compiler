//! Diagnostic code registry: metadata the renderers attach to a code.
//!
//! Codes are grouped in families, one file per family, so a slice that adds codes edits only its
//! own family file:
//!
//! - [`parse`]: `E0101`–`E0199`, parse errors.
//! - [`semantic`]: `E1001`–`E1799`, semantic errors.
//! - [`project`]: `E1801`–`E1900`, Mod, manifest, glue and query-bound errors.
//! - [`macros`]: `E1901`–`E1999`, macro expansion errors.
//! - [`internal`]: `E2101`–`E2199`, internal compiler errors.
//! - [`project_files`]: `E3001`–`E3099`, project files, lock files and materialization.
//! - [`lints`]: `W1000`–`W1999`, style, documentation and naming warnings.
//! - [`unused`]: unused and unreachable code; every entry carries [`DiagnosticTag::Unnecessary`].
//! - [`deprecated`]: uses of deprecated items; every entry carries [`DiagnosticTag::Deprecated`].
//!
//! The registry is the one table of codes: every code the compiler emits has an entry (a test scans
//! the compiler sources), and codes with a name are spelled only here as constants. Mod analyzers
//! report their own codes and are not registered. Every code links to the one registry page,
//! [`REGISTRY_URL`]; per-code pages are planned for 0.7.

pub mod deprecated;
pub mod internal;
pub mod lints;
pub mod macros;
pub mod parse;
pub mod project;
pub mod project_files;
pub mod semantic;
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
pub const FAMILIES: &[CodeFamily] = &[
    parse::FAMILY,
    semantic::FAMILY,
    project::FAMILY,
    macros::FAMILY,
    internal::FAMILY,
    project_files::FAMILY,
    lints::FAMILY,
    unused::FAMILY,
    deprecated::FAMILY,
];

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

    #[test]
    fn unused_item_and_the_internal_error_are_separate_entries() {
        let unused = lookup("W2101").expect("W2101");
        let internal = lookup(internal::SEMANTIC_FACT_UNAVAILABLE).expect("E2101");
        assert_eq!(unused.summary, "unused item");
        assert!(internal.summary.starts_with("internal error"));
        assert!(unused::CODES.iter().all(|info| info.code != "E2101"));
        assert!(internal::CODES.iter().all(|info| info.code != "W2101"));
        assert!(lookup("E0111").is_none(), "the unexpected tree shape is internal");
        assert_eq!(lookup(internal::UNEXPECTED_TREE).map(|info| info.code), Some("E2106"));
    }

    /// Non-test Rust sources of the compiler crates (test crates, `tests/` directories, test files
    /// and `#[cfg(test)]` tails excluded; the registry itself excluded).
    fn compiler_sources() -> Vec<(std::path::PathBuf, String)> {
        fn walk(directory: &std::path::Path, out: &mut Vec<(std::path::PathBuf, String)>) {
            let Ok(entries) = std::fs::read_dir(directory) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
                if path.is_dir() {
                    if name != "tests" && name != "target" && name != "diagnostic_codes" {
                        walk(&path, out);
                    }
                } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    let text = text.split("#[cfg(test)]").next().unwrap_or_default().to_string();
                    out.push((path, text));
                }
            }
        }
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crates directory");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(crates).expect("crates").flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("beskid_tests_") || name == "beskid_e2e_tests" {
                continue;
            }
            walk(&entry.path().join("src"), &mut out);
        }
        assert!(out.len() > 100, "compiler sources were found");
        out
    }

    /// Every `"E####"` / `"W####"` literal in a source text.
    fn code_literals(text: &str) -> Vec<&str> {
        let bytes = text.as_bytes();
        (0..bytes.len().saturating_sub(6))
            .filter(|&index| bytes[index] == b'"' && bytes[index + 6] == b'"')
            .map(|index| &text[index + 1..index + 6])
            .filter(|code| is_registry_code(code))
            .collect()
    }

    #[test]
    fn every_code_the_compiler_emits_is_registered() {
        let mut missing = Vec::new();
        for (path, text) in compiler_sources() {
            for code in code_literals(&text) {
                if lookup(code).is_none() {
                    missing.push(format!("{code} in {}", path.display()));
                }
            }
        }
        assert!(missing.is_empty(), "unregistered codes:\n{}", missing.join("\n"));
    }

    #[test]
    fn no_diagnostic_uses_a_free_form_code() {
        let mut free_form = Vec::new();
        for (path, text) in compiler_sources() {
            for marker in ["code: Some(\"", "Some(\""] {
                for (index, _) in text.match_indices(marker) {
                    let rest = &text[index + marker.len()..];
                    let Some(end) = rest.find('"') else { continue };
                    let literal = &rest[..end];
                    let after = rest[end + 1..].trim_start_matches(".to_string()").trim_start_matches(".into()");
                    let after = after.trim_start_matches(')').trim_start_matches(',').trim_start();
                    let is_code_argument = marker.starts_with("code") || after.starts_with("Severity::")
                        || after.starts_with("crate::analysis::Severity::");
                    if is_code_argument && !is_registry_code(literal) {
                        free_form.push(format!("`{literal}` in {}", path.display()));
                    }
                }
            }
        }
        assert!(free_form.is_empty(), "diagnostic codes outside the registry:\n{}", free_form.join("\n"));
    }
}
