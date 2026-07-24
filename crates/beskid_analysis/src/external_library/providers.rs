//! Concrete `ExternalLibrary` providers shipped with the v0.3 closed registry.
//!
//! Tier-1 hosts ship a C / POSIX provider (`c-posix`). A POSIX-only alias (`posix`) is also
//! registered to make `pthread` / threading-only resolutions explicit; both run the same mapping
//! logic for now.

use std::path::PathBuf;

use super::error::LibraryResolveError;
use super::trait_def::ExternalLibrary;

/// Built-in C / POSIX provider (`id() == "c-posix"`).
///
/// Maps a small closed set of POSIX system libraries to platform linker arguments. The set is
/// intentionally small (per v0.3 spec scope) and is normalized over canonical `lib*` aliases.
#[derive(Debug, Default, Clone, Copy)]
pub struct CPosixProvider;

impl ExternalLibrary for CPosixProvider {
    fn id(&self) -> &'static str {
        "c-posix"
    }

    fn host_key(&self) -> &str {
        "posix"
    }

    fn resolve_link_args(&self, logical: &str) -> Result<Vec<String>, LibraryResolveError> {
        resolve_c_posix(self.id(), self.host_key(), logical)
    }

    fn resolve_search_paths(&self, logical: &str) -> Vec<PathBuf> {
        resolve_path_like_search_paths(logical)
    }
}

/// POSIX provider alias (`id() == "posix"`).
///
/// Same closed mapping as [`CPosixProvider`] today; reserved to differentiate POSIX-only
/// threading / scheduler-related logical names from a libc-centric provider in future revisions.
#[derive(Debug, Default, Clone, Copy)]
pub struct PosixProvider;

impl ExternalLibrary for PosixProvider {
    fn id(&self) -> &'static str {
        "posix"
    }

    fn host_key(&self) -> &str {
        "posix"
    }

    fn resolve_link_args(&self, logical: &str) -> Result<Vec<String>, LibraryResolveError> {
        resolve_c_posix(self.id(), self.host_key(), logical)
    }

    fn resolve_search_paths(&self, logical: &str) -> Vec<PathBuf> {
        resolve_path_like_search_paths(logical)
    }
}

fn resolve_c_posix(provider: &str, host: &str, logical: &str) -> Result<Vec<String>, LibraryResolveError> {
    let logical_trimmed = logical.trim();
    if logical_trimmed.is_empty() {
        return Err(LibraryResolveError::InvalidLogicalName {
            logical: logical.to_string(),
            detail: "logical name must be non-empty".to_string(),
        });
    }

    // Pre-formed linker flag pass-through (e.g. `-lm`).
    if let Some(stripped) = logical_trimmed.strip_prefix("-l")
        && !stripped.is_empty()
        && stripped.chars().all(is_link_short_name_char)
    {
        return Ok(vec![logical_trimmed.to_string()]);
    }

    let canonical = canonical_logical_name(logical_trimmed);
    if let Some(args) = closed_c_posix_mapping(&canonical) {
        return Ok(args);
    }

    // Absolute path / sandbox SDK input passes through unchanged.
    if looks_like_path(logical_trimmed) {
        return Ok(vec![logical_trimmed.to_string()]);
    }

    Err(LibraryResolveError::UnknownLogicalName {
        provider: provider.to_string(),
        host: host.to_string(),
        logical: logical.to_string(),
        detail: format!("v0.3 closed registry only ships C / POSIX names (known: {})", known_logical_names_csv()),
    })
}

fn resolve_path_like_search_paths(logical: &str) -> Vec<PathBuf> {
    let trimmed = logical.trim();
    if !looks_like_path(trimmed) {
        return Vec::new();
    }
    let path = PathBuf::from(trimmed);
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => vec![parent.to_path_buf()],
        _ => Vec::new(),
    }
}

fn looks_like_path(logical: &str) -> bool {
    logical.starts_with('/') || logical.contains(std::path::MAIN_SEPARATOR) || logical.contains('.')
}

/// Closed, intentional set of supported C / POSIX logical names.
///
/// The v0.3 spec keeps this small; expanding requires a new ADR.
const C_POSIX_LOGICAL_NAMES: &[(&str, &[&str])] = &[
    ("c", &["-lc"]),
    ("m", &["-lm"]),
    ("pthread", &["-lpthread"]),
    ("dl", &["-ldl"]),
    ("rt", &["-lrt"]),
    ("util", &["-lutil"]),
    ("crypt", &["-lcrypt"]),
    ("resolv", &["-lresolv"]),
];

fn closed_c_posix_mapping(canonical: &str) -> Option<Vec<String>> {
    C_POSIX_LOGICAL_NAMES
        .iter()
        .find(|(name, _)| *name == canonical)
        .map(|(_, args)| args.iter().map(|s| (*s).to_string()).collect())
}

fn canonical_logical_name(logical: &str) -> String {
    let lower = logical.to_ascii_lowercase();
    let stripped_prefix = lower.strip_prefix("lib").unwrap_or(&lower);
    let stripped_suffix = stripped_prefix
        .strip_suffix(".so")
        .or_else(|| stripped_prefix.strip_suffix(".dylib"))
        .or_else(|| stripped_prefix.strip_suffix(".a"))
        .unwrap_or(stripped_prefix);
    stripped_suffix.to_string()
}

fn known_logical_names_csv() -> String {
    C_POSIX_LOGICAL_NAMES.iter().map(|(name, _)| *name).collect::<Vec<&str>>().join(", ")
}

fn is_link_short_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '+'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_posix_provider_resolves_libc() {
        let provider = CPosixProvider;
        assert_eq!(provider.id(), "c-posix");
        let args = provider.resolve_link_args("libc").expect("libc");
        assert_eq!(args, vec!["-lc"]);
    }

    #[test]
    fn c_posix_provider_canonicalizes_extensions() {
        let provider = CPosixProvider;
        assert_eq!(provider.resolve_link_args("libc.so").unwrap(), vec!["-lc"]);
        assert_eq!(provider.resolve_link_args("libpthread.dylib").unwrap(), vec!["-lpthread"]);
    }

    #[test]
    fn c_posix_provider_rejects_unknown_logical() {
        let provider = CPosixProvider;
        let err = provider.resolve_link_args("totally-not-a-real-libname").expect_err("unknown name");
        match err {
            LibraryResolveError::UnknownLogicalName { provider, .. } => {
                assert_eq!(provider, "c-posix");
            }
            other => panic!("expected UnknownLogicalName, got {other:?}"),
        }
    }

    #[test]
    fn c_posix_provider_passthrough_for_flags() {
        let provider = CPosixProvider;
        assert_eq!(provider.resolve_link_args("-lm").unwrap(), vec!["-lm"]);
    }

    #[test]
    fn c_posix_provider_passthrough_for_paths() {
        let provider = CPosixProvider;
        let args = provider.resolve_link_args("/usr/lib/libfoo.so").expect("path passthrough");
        assert_eq!(args, vec!["/usr/lib/libfoo.so"]);
        let paths = provider.resolve_search_paths("/usr/lib/libfoo.so");
        assert_eq!(paths, vec![PathBuf::from("/usr/lib")]);
    }

    #[test]
    fn empty_logical_rejected() {
        let provider = CPosixProvider;
        let err = provider.resolve_link_args("  ").expect_err("empty");
        assert!(matches!(err, LibraryResolveError::InvalidLogicalName { .. }));
    }
}
