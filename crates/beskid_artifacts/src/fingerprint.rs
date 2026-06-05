//! Content-addressed unit fingerprints (path-independent).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Shared grammar/compiler revision for cache invalidation (workspace constant).
pub fn grammar_revision() -> &'static str {
    beskid_pipeline::GRAMMAR_REVISION
}

/// Fingerprint from source bytes + grammar revision (not absolute path).
pub fn content_fingerprint(source: &str) -> String {
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    grammar_revision().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
