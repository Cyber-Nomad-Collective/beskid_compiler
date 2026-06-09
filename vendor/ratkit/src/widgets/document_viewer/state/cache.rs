//! Lightweight render cache marker for document viewers.

/// Stores the content hash used by the latest render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheState {
    /// Hash of the last rendered content and display settings.
    pub content_hash: u64,
}

impl CacheState {
    /// Replaces the cached content hash.
    pub fn set_content_hash(&mut self, content_hash: u64) {
        self.content_hash = content_hash;
    }
}
