//! Unit artifact cache stats and legacy fingerprint helpers (disk layout retired; see `beskid_artifacts`).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

pub use beskid_artifacts::content_fingerprint as unit_content_fingerprint;
pub use beskid_artifacts::grammar_revision;
pub use beskid_artifacts::persistence::cache_root_for_project;

static DISK_HITS: AtomicUsize = AtomicUsize::new(0);
static DISK_MISSES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Default, Clone, Copy)]
pub struct UnitCacheStats {
    pub hits: usize,
    pub misses: usize,
}

pub fn record_disk_hit() {
    DISK_HITS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_disk_miss() {
    DISK_MISSES.fetch_add(1, Ordering::Relaxed);
}

pub fn disk_cache_stats() -> UnitCacheStats {
    UnitCacheStats {
        hits: DISK_HITS.load(Ordering::Relaxed),
        misses: DISK_MISSES.load(Ordering::Relaxed),
    }
}

/// Legacy path-bound fingerprint (tests only); production uses [`unit_content_fingerprint`].
pub fn unit_fingerprint(path: &Path, source: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    source.hash(&mut hasher);
    grammar_revision().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// No-op: legacy `units/` manifest retired in favor of `obj/beskid/cache/salsa/`.
pub fn ensure_manifest(_project_root: &Path) -> std::io::Result<()> {
    Ok(())
}
