//! Content-addressed unit fingerprints (path-independent).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Content identity of the grammar and syntax-tree construction (`beskid_pipeline::GRAMMAR_REVISION`):
/// the package version plus a digest of the sources that shape parse trees. Persisted artifacts
/// written under another revision are invalidated, never read.
pub fn grammar_revision() -> &'static str {
    beskid_pipeline::GRAMMAR_REVISION
}

/// Fingerprint from source bytes + grammar revision (not absolute path).
pub fn content_fingerprint(source: &str) -> String {
    fingerprint_under(source, grammar_revision())
}

fn fingerprint_under(source: &str, grammar_revision: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    grammar_revision.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_changes_when_the_grammar_revision_changes() {
        let source = "i64 Main() { return 0_i64; }";
        let current = content_fingerprint(source);
        assert_eq!(current, fingerprint_under(source, grammar_revision()));
        let other_revision = format!("{}-other", grammar_revision());
        assert_ne!(current, fingerprint_under(source, &other_revision));
        let (version, _) = grammar_revision().split_once('+').expect("<version>+<digest>");
        assert_ne!(current, fingerprint_under(source, version), "the bare package version is a different revision");
    }

    #[test]
    fn grammar_revision_is_version_plus_content_digest() {
        let (version, digest) = grammar_revision().split_once('+').expect("<version>+<digest>");
        assert!(!version.is_empty() && version.split('.').all(|part| part.parse::<u64>().is_ok()), "{version}");
        assert!(digest.len() >= 16 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()), "{digest}");
    }
}
